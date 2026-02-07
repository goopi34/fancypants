#ifndef RANGE_SERVICE_H
#define RANGE_SERVICE_H

#include <zephyr/types.h>
#include <zephyr/bluetooth/conn.h>
#include <zephyr/bluetooth/uuid.h>
#include <zephyr/bluetooth/gatt.h>

#ifdef __cplusplus
extern "C" {
#endif

/*
 * Custom Range Service
 *
 * UUID base: 00000000-7272-6e67-6669-6e6465720000
 *            (ASCII "rrngfinder" embedded)
 *
 * Service UUID:       00000001-7272-6e67-6669-6e6465720000
 * Range Char UUID:    00000002-7272-6e67-6669-6e6465720000
 *   - Notify: uint16_t distance in mm (little-endian)
 *   - Read:   last known distance in mm
 * Config Char UUID:   00000003-7272-6e67-6669-6e6465720000
 *   - Read/Write: configuration struct
 */

/* Service UUID */
#define RANGE_SERVICE_UUID_VAL \
	BT_UUID_128_ENCODE(0x00000001, 0x7272, 0x6e67, 0x6669, 0x6e6465720000)
#define RANGE_SERVICE_UUID BT_UUID_DECLARE_128(RANGE_SERVICE_UUID_VAL)

/* Range measurement characteristic */
#define RANGE_CHAR_UUID_VAL \
	BT_UUID_128_ENCODE(0x00000002, 0x7272, 0x6e67, 0x6669, 0x6e6465720000)
#define RANGE_CHAR_UUID BT_UUID_DECLARE_128(RANGE_CHAR_UUID_VAL)

/* Configuration characteristic */
#define RANGE_CONFIG_CHAR_UUID_VAL \
	BT_UUID_128_ENCODE(0x00000003, 0x7272, 0x6e67, 0x6669, 0x6e6465720000)
#define RANGE_CONFIG_CHAR_UUID BT_UUID_DECLARE_128(RANGE_CONFIG_CHAR_UUID_VAL)

/* Configuration struct written/read via BLE */
struct range_config {
	uint16_t sample_interval_ms;  /* sensor polling rate */
	uint16_t notify_interval_ms;  /* BLE notification rate */
	uint16_t max_range_mm;        /* clamp: ignore readings above this */
	uint16_t min_range_mm;        /* clamp: ignore readings below this */
} __packed;

/**
 * @brief Initialize the Range Service and register GATT attributes.
 * @return 0 on success, negative errno on failure.
 */
int range_service_init(void);

/**
 * @brief Update the range measurement and send notification if subscribed.
 * @param distance_mm Distance in millimeters from the VL53L0X.
 * @return 0 on success, negative errno on failure.
 */
int range_service_update(uint16_t distance_mm);

/**
 * @brief Get the current configuration.
 * @return Pointer to the active config.
 */
const struct range_config *range_service_get_config(void);

#ifdef __cplusplus
}
#endif

#endif /* RANGE_SERVICE_H */
