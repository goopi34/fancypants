#include "range_service.h"

#include <zephyr/kernel.h>
#include <zephyr/logging/log.h>
#include <zephyr/bluetooth/bluetooth.h>
#include <zephyr/bluetooth/gatt.h>

LOG_MODULE_REGISTER(range_svc, LOG_LEVEL_INF);

/* Current range reading */
static uint16_t current_range_mm;

/* Active configuration - defaults match prj.conf */
static struct range_config active_config = {
	.sample_interval_ms = CONFIG_RANGE_SAMPLE_INTERVAL_MS,
	.notify_interval_ms = CONFIG_RANGE_NOTIFY_INTERVAL_MS,
	.max_range_mm = 1200,  /* VL53L0X max useful range */
	.min_range_mm = 30,    /* VL53L0X min range */
};

/* Track CCC subscription state */
static bool range_notify_enabled;

/* CCC changed callback */
static void range_ccc_changed(const struct bt_gatt_attr *attr, uint16_t value)
{
	range_notify_enabled = (value == BT_GATT_CCC_NOTIFY);
	LOG_INF("Range notifications %s", range_notify_enabled ? "enabled" : "disabled");
}

/* Read handler for range characteristic */
static ssize_t read_range(struct bt_conn *conn,
			   const struct bt_gatt_attr *attr,
			   void *buf, uint16_t len, uint16_t offset)
{
	return bt_gatt_attr_read(conn, attr, buf, len, offset,
				 &current_range_mm, sizeof(current_range_mm));
}

/* Read handler for config characteristic */
static ssize_t read_config(struct bt_conn *conn,
			    const struct bt_gatt_attr *attr,
			    void *buf, uint16_t len, uint16_t offset)
{
	return bt_gatt_attr_read(conn, attr, buf, len, offset,
				 &active_config, sizeof(active_config));
}

/* Write handler for config characteristic */
static ssize_t write_config(struct bt_conn *conn,
			     const struct bt_gatt_attr *attr,
			     const void *buf, uint16_t len,
			     uint16_t offset, uint8_t flags)
{
	if (offset + len > sizeof(active_config)) {
		return BT_GATT_ERR(BT_ATT_ERR_INVALID_OFFSET);
	}

	if (len != sizeof(active_config)) {
		return BT_GATT_ERR(BT_ATT_ERR_INVALID_ATTRIBUTE_LEN);
	}

	const struct range_config *new_config = buf;

	/* Validate ranges */
	if (new_config->sample_interval_ms < 10 ||
	    new_config->sample_interval_ms > 5000) {
		LOG_WRN("Rejected config: sample_interval_ms=%u out of range",
			new_config->sample_interval_ms);
		return BT_GATT_ERR(BT_ATT_ERR_VALUE_NOT_ALLOWED);
	}

	if (new_config->notify_interval_ms < 10 ||
	    new_config->notify_interval_ms > 5000) {
		LOG_WRN("Rejected config: notify_interval_ms=%u out of range",
			new_config->notify_interval_ms);
		return BT_GATT_ERR(BT_ATT_ERR_VALUE_NOT_ALLOWED);
	}

	if (new_config->min_range_mm >= new_config->max_range_mm) {
		LOG_WRN("Rejected config: min_range >= max_range");
		return BT_GATT_ERR(BT_ATT_ERR_VALUE_NOT_ALLOWED);
	}

	memcpy(&active_config, new_config, sizeof(active_config));
	LOG_INF("Config updated: sample=%ums notify=%ums range=[%u-%u]mm",
		active_config.sample_interval_ms,
		active_config.notify_interval_ms,
		active_config.min_range_mm,
		active_config.max_range_mm);

	return len;
}

/* GATT Service Declaration */
BT_GATT_SERVICE_DEFINE(range_svc,
	BT_GATT_PRIMARY_SERVICE(RANGE_SERVICE_UUID),

	/* Range measurement: read + notify */
	BT_GATT_CHARACTERISTIC(RANGE_CHAR_UUID,
			       BT_GATT_CHRC_READ | BT_GATT_CHRC_NOTIFY,
			       BT_GATT_PERM_READ,
			       read_range, NULL, &current_range_mm),
	BT_GATT_CCC(range_ccc_changed,
		     BT_GATT_PERM_READ | BT_GATT_PERM_WRITE),

	/* Configuration: read + write */
	BT_GATT_CHARACTERISTIC(RANGE_CONFIG_CHAR_UUID,
			       BT_GATT_CHRC_READ | BT_GATT_CHRC_WRITE,
			       BT_GATT_PERM_READ | BT_GATT_PERM_WRITE,
			       read_config, write_config, &active_config),
);

int range_service_init(void)
{
	LOG_INF("Range Service initialized");
	return 0;
}

int range_service_update(uint16_t distance_mm)
{
	current_range_mm = distance_mm;

	if (!range_notify_enabled) {
		return 0;
	}

	return bt_gatt_notify(NULL, &range_svc.attrs[1],
			      &current_range_mm, sizeof(current_range_mm));
}

const struct range_config *range_service_get_config(void)
{
	return &active_config;
}
