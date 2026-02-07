#ifndef BATTERY_H
#define BATTERY_H

#include <zephyr/types.h>

#ifdef __cplusplus
extern "C" {
#endif

/**
 * @brief Initialize battery ADC reading.
 * @return 0 on success, negative errno on failure.
 */
int battery_init(void);

/**
 * @brief Read current battery voltage in millivolts.
 * @return Battery voltage in mV, or negative errno on failure.
 */
int battery_read_mv(void);

/**
 * @brief Convert millivolt reading to percentage (0-100).
 *
 * Uses a LiPo discharge curve approximation:
 *   4200mV = 100%, 3700mV = 50%, 3300mV = 0%
 *
 * @param mv Battery voltage in millivolts.
 * @return Percentage 0-100.
 */
uint8_t battery_mv_to_pct(int mv);

#ifdef __cplusplus
}
#endif

#endif /* BATTERY_H */