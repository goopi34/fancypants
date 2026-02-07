/*
 * Rangefinder BLE Peripheral
 *
 * Reads distance from a VL53L0X time-of-flight sensor via I2C and
 * broadcasts it over BLE using a custom GATT service. Also reports
 * battery level via the standard Battery Service (BAS).
 *
 * Target: Adafruit Feather nRF52840 Express
 * Sensor: Adafruit VL53L0X breakout (I2C addr 0x29)
 *
 * BLE Services:
 *   - Custom Range Service (notify distance_mm + config)
 *   - Battery Service (BAS, standard)
 *   - Device Information Service (optional, via Kconfig)
 */

#include <zephyr/kernel.h>
#include <zephyr/device.h>
#include <zephyr/drivers/sensor.h>
#include <zephyr/bluetooth/bluetooth.h>
#include <zephyr/bluetooth/gap.h>
#include <zephyr/bluetooth/gatt.h>
#include <zephyr/bluetooth/services/bas.h>
#include <zephyr/logging/log.h>

#include "range_service.h"
#include "battery.h"

LOG_MODULE_REGISTER(main, LOG_LEVEL_INF);

/* VL53L0X device from devicetree */
static const struct device *range_sensor;

/* Connection state */
static struct bt_conn *current_conn;

/* Advertising data */
static const struct bt_data ad[] = {
	BT_DATA_BYTES(BT_DATA_FLAGS, (BT_LE_AD_GENERAL | BT_LE_AD_NO_BREDR)),
	BT_DATA_BYTES(BT_DATA_UUID128_ALL, RANGE_SERVICE_UUID_VAL),
};

/* Scan response - device name */
static const struct bt_data sd[] = {
	BT_DATA(BT_DATA_NAME_COMPLETE, CONFIG_BT_DEVICE_NAME,
		sizeof(CONFIG_BT_DEVICE_NAME) - 1),
};

/* BLE connection callbacks */
static void connected(struct bt_conn *conn, uint8_t err)
{
	if (err) {
		LOG_ERR("Connection failed (err %u)", err);
		return;
	}

	LOG_INF("Connected");
	current_conn = bt_conn_ref(conn);
}

static void disconnected(struct bt_conn *conn, uint8_t reason)
{
	LOG_INF("Disconnected (reason %u)", reason);

	if (current_conn) {
		bt_conn_unref(current_conn);
		current_conn = NULL;
	}

	/* Restart advertising */
	int err = bt_le_adv_start(BT_LE_ADV_CONN, ad, ARRAY_SIZE(ad),
				  sd, ARRAY_SIZE(sd));
	if (err) {
		LOG_ERR("Advertising restart failed (err %d)", err);
	}
}

BT_CONN_CB_DEFINE(conn_callbacks) = {
	.connected = connected,
	.disconnected = disconnected,
};

/* Read VL53L0X and return distance in mm, or negative on error */
static int read_range_mm(void)
{
	struct sensor_value val;
	int ret;

	ret = sensor_sample_fetch(range_sensor);
	if (ret < 0) {
		LOG_WRN("Sensor fetch failed: %d", ret);
		return ret;
	}

	ret = sensor_channel_get(range_sensor, SENSOR_CHAN_DISTANCE, &val);
	if (ret < 0) {
		LOG_WRN("Sensor channel get failed: %d", ret);
		return ret;
	}

	/*
	 * Zephyr's VL53L0X driver returns distance in meters as
	 * a sensor_value (val1 = integer meters, val2 = fractional in
	 * millionths). Convert to mm.
	 */
	int distance_mm = val.val1 * 1000 + val.val2 / 1000;

	return distance_mm;
}

/* Sensor polling thread */
static void sensor_thread_fn(void *p1, void *p2, void *p3)
{
	ARG_UNUSED(p1);
	ARG_UNUSED(p2);
	ARG_UNUSED(p3);

	LOG_INF("Sensor thread started");

	while (1) {
		const struct range_config *cfg = range_service_get_config();

		int distance = read_range_mm();
		if (distance >= 0) {
			/* Clamp to configured range */
			uint16_t clamped = (uint16_t)distance;
			if (clamped < cfg->min_range_mm) {
				clamped = cfg->min_range_mm;
			}
			if (clamped > cfg->max_range_mm) {
				clamped = cfg->max_range_mm;
			}

			range_service_update(clamped);
		}

		k_msleep(cfg->sample_interval_ms);
	}
}

/* Battery monitoring thread */
static void battery_thread_fn(void *p1, void *p2, void *p3)
{
	ARG_UNUSED(p1);
	ARG_UNUSED(p2);
	ARG_UNUSED(p3);

	LOG_INF("Battery thread started");

	while (1) {
		int mv = battery_read_mv();
		if (mv > 0) {
			uint8_t pct = battery_mv_to_pct(mv);
			bt_bas_set_battery_level(pct);
			LOG_DBG("Battery: %dmV (%u%%)", mv, pct);
		}

		k_sleep(K_SECONDS(CONFIG_BATTERY_SAMPLE_INTERVAL_S));
	}
}

/* Thread stacks */
K_THREAD_STACK_DEFINE(sensor_stack, 2048);
K_THREAD_STACK_DEFINE(battery_stack, 1024);

static struct k_thread sensor_thread;
static struct k_thread battery_thread;

int main(void)
{
	int err;

	LOG_INF("Rangefinder BLE starting...");

	/* Get VL53L0X device */
	range_sensor = DEVICE_DT_GET_ONE(st_vl53l0x);
	if (!device_is_ready(range_sensor)) {
		LOG_ERR("VL53L0X sensor not ready");
		return -ENODEV;
	}
	LOG_INF("VL53L0X sensor ready");

	/* Initialize battery ADC */
	err = battery_init();
	if (err) {
		LOG_WRN("Battery init failed: %d (continuing without battery)", err);
	}

	/* Initialize custom range service */
	err = range_service_init();
	if (err) {
		LOG_ERR("Range service init failed: %d", err);
		return err;
	}

	/* Enable Bluetooth */
	err = bt_enable(NULL);
	if (err) {
		LOG_ERR("Bluetooth init failed: %d", err);
		return err;
	}
	LOG_INF("Bluetooth initialized");

	/* Start advertising */
	err = bt_le_adv_start(BT_LE_ADV_CONN, ad, ARRAY_SIZE(ad),
			      sd, ARRAY_SIZE(sd));
	if (err) {
		LOG_ERR("Advertising failed to start: %d", err);
		return err;
	}
	LOG_INF("Advertising as \"%s\"", CONFIG_BT_DEVICE_NAME);

	/* Spawn worker threads */
	k_thread_create(&sensor_thread, sensor_stack,
			K_THREAD_STACK_SIZEOF(sensor_stack),
			sensor_thread_fn, NULL, NULL, NULL,
			K_PRIO_COOP(7), 0, K_NO_WAIT);
	k_thread_name_set(&sensor_thread, "sensor");

	k_thread_create(&battery_thread, battery_stack,
			K_THREAD_STACK_SIZEOF(battery_stack),
			battery_thread_fn, NULL, NULL, NULL,
			K_PRIO_PREEMPT(10), 0, K_NO_WAIT);
	k_thread_name_set(&battery_thread, "battery");

	LOG_INF("Rangefinder BLE running");

	return 0;
}
