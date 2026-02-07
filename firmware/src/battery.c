#include "battery.h"

#include <zephyr/kernel.h>
#include <zephyr/drivers/adc.h>
#include <zephyr/logging/log.h>

LOG_MODULE_REGISTER(battery, LOG_LEVEL_INF);

/*
 * Adafruit Feather nRF52840 has a voltage divider on the VBAT pin:
 *   VBAT --- [100K] --- ADC (P0.29, AIN5) --- [100K] --- GND
 *
 * So ADC reads VBAT / 2. We need to multiply by 2 to get actual voltage.
 * nRF52840 ADC internal reference = 0.6V, gain 1/6 = 3.6V full scale.
 */

#define ADC_RESOLUTION    12
#define ADC_GAIN          ADC_GAIN_1_6
#define ADC_REFERENCE     ADC_REF_INTERNAL    /* 0.6V */
#define ADC_CHANNEL       5                    /* AIN5 = P0.29 */
#define DIVIDER_RATIO     2                    /* VBAT/2 resistor divider */

/* Full scale voltage in mV: 0.6V * 6 (gain) = 3.6V = 3600mV */
#define ADC_FULL_SCALE_MV 3600

static const struct device *adc_dev;
static int16_t adc_buffer;

static struct adc_channel_cfg channel_cfg = {
	.gain = ADC_GAIN,
	.reference = ADC_REFERENCE,
	.acquisition_time = ADC_ACQ_TIME(ADC_ACQ_TIME_MICROSECONDS, 40),
	.channel_id = ADC_CHANNEL,
#if defined(CONFIG_ADC_NRFX_SAADC)
	.input_positive = SAADC_CH_PSELP_PSELP_AnalogInput5,
#endif
};

static struct adc_sequence sequence = {
	.channels = BIT(ADC_CHANNEL),
	.buffer = &adc_buffer,
	.buffer_size = sizeof(adc_buffer),
	.resolution = ADC_RESOLUTION,
};

int battery_init(void)
{
	adc_dev = DEVICE_DT_GET(DT_NODELABEL(adc));
	if (!device_is_ready(adc_dev)) {
		LOG_ERR("ADC device not ready");
		return -ENODEV;
	}

	int ret = adc_channel_setup(adc_dev, &channel_cfg);
	if (ret < 0) {
		LOG_ERR("ADC channel setup failed: %d", ret);
		return ret;
	}

	LOG_INF("Battery ADC initialized on AIN%d", ADC_CHANNEL);
	return 0;
}

int battery_read_mv(void)
{
	int ret = adc_read(adc_dev, &sequence);
	if (ret < 0) {
		LOG_ERR("ADC read failed: %d", ret);
		return ret;
	}

	/* Convert raw ADC to millivolts */
	int32_t mv = adc_buffer;

	/* Handle negative values from SAADC offset */
	if (mv < 0) {
		mv = 0;
	}

	/* Scale: (raw / 4096) * 3600mV * 2 (divider) */
	mv = (mv * ADC_FULL_SCALE_MV * DIVIDER_RATIO) / (1 << ADC_RESOLUTION);

	return (int)mv;
}

uint8_t battery_mv_to_pct(int mv)
{
	/* LiPo discharge curve approximation (single-cell 3.7V nominal) */
	if (mv >= 4200) {
		return 100;
	} else if (mv >= 4100) {
		/* 4200-4100: 100-90% */
		return 90 + (mv - 4100) * 10 / 100;
	} else if (mv >= 3800) {
		/* 4100-3800: 90-50% */
		return 50 + (mv - 3800) * 40 / 300;
	} else if (mv >= 3600) {
		/* 3800-3600: 50-20% */
		return 20 + (mv - 3600) * 30 / 200;
	} else if (mv >= 3300) {
		/* 3600-3300: 20-0% */
		return (mv - 3300) * 20 / 300;
	} else {
		return 0;
	}
}
