(() => {
  const ACTUATOR_TYPES = ["Vibrate", "Rotate", "Oscillate", "Constrict", "Inflate", "Position"];
  const MAX_TRACK_MM = 800;

  const tauri = window.__TAURI__;
  const invoke = tauri?.core?.invoke
    ? (cmd, args) => tauri.core.invoke(cmd, args)
    : async (cmd) => {
        console.warn("Tauri bridge not detected, running in browser preview mode:", cmd);
        return null;
      };
  const listen = tauri?.event?.listen
    ? (ev, cb) => tauri.event.listen(ev, cb)
    : async () => {};

  let currentConfig = {
    ble: {
      device_name: "Fancypants",
      scan_timeout_secs: 30,
      reconnect_delay_secs: 5,
      data_timeout_secs: 5,
      sample_interval_ms: null,
      notify_interval_ms: null,
    },
    mapping: {
      mode: "distance",
      invert: true,
      max_speed_mm_s: 500.0,
      min_range_mm: 30,
      max_range_mm: 300,
      min_intensity: 0.0,
      max_intensity: 1.0,
      deadzone_mm: 500,
      smoothing: 0.3,
    },
    buttplug: {
      mode: "intiface",
      server_address: "ws://127.0.0.1:12345",
      device_index: null,
      actuator_types: [{ type: "Vibrate", min_intensity: 0.0, max_intensity: 1.0, pattern: "constant", pulse_hz: 1.0 }],
      devices: [],
    },
  };

  let sessionRunning = false;

  // DOM Elements
  const statusDot = document.getElementById("status-dot");
  const statusText = document.getElementById("status-text");
  const batteryPill = document.getElementById("battery-pill");
  const batteryValue = document.getElementById("battery-value");
  const btnSession = document.getElementById("btn-session");
  const toastEl = document.getElementById("toast");

  const liveModeBadge = document.getElementById("live-mode-badge");
  const metricDistance = document.getElementById("metric-distance");
  const metricIntensity = document.getElementById("metric-intensity");
  const labelActiveWindow = document.getElementById("label-active-window");
  const labelDeadzone = document.getElementById("label-deadzone");
  const labelSmoothing = document.getElementById("label-smoothing");
  const rangeActiveZone = document.getElementById("range-active-zone");
  const rangeDeadzoneLine = document.getElementById("range-deadzone-line");
  const rangeCursor = document.getElementById("range-cursor");
  const intensityFill = document.getElementById("intensity-fill");
  const liveToysList = document.getElementById("live-toys-list");

  const modeStandaloneBtn = document.getElementById("mode-standalone");
  const modeIntifaceBtn = document.getElementById("mode-intiface");
  const intifaceAddressRow = document.getElementById("intiface-address-row");
  const inputServerAddress = document.getElementById("input-server-address");
  const inputBleName = document.getElementById("input-ble-name");
  const inputDataTimeout = document.getElementById("input-data-timeout");
  const inputSampleMs = document.getElementById("input-sample-ms");
  const inputNotifyMs = document.getElementById("input-notify-ms");

  const mapModeDistanceBtn = document.getElementById("map-mode-distance");
  const mapModeVelocityBtn = document.getElementById("map-mode-velocity");
  const distanceControls = document.getElementById("distance-controls");
  const velocityControls = document.getElementById("velocity-controls");
  const inputInvert = document.getElementById("input-invert");
  const inputMinRange = document.getElementById("input-min-range");
  const inputMaxRange = document.getElementById("input-max-range");
  const inputMaxSpeed = document.getElementById("input-max-speed");
  const inputDeadzone = document.getElementById("input-deadzone");
  const inputSmoothing = document.getElementById("input-smoothing");
  const inputMinIntensity = document.getElementById("input-min-intensity");
  const inputMaxIntensity = document.getElementById("input-max-intensity");

  const valMinRange = document.getElementById("val-min-range");
  const valMaxRange = document.getElementById("val-max-range");
  const valMaxSpeed = document.getElementById("val-max-speed");
  const valDeadzone = document.getElementById("val-deadzone");
  const valSmoothing = document.getElementById("val-smoothing");
  const valMinIntensity = document.getElementById("val-min-intensity");
  const valMaxIntensity = document.getElementById("val-max-intensity");

  const toysContainer = document.getElementById("toys-container");
  const btnAddToy = document.getElementById("btn-add-toy");
  const btnSaveConfig = document.getElementById("btn-save-config");
  const saveStatus = document.getElementById("save-status");

  function showToast(msg) {
    if (!msg) {
      toastEl.classList.add("hidden");
      return;
    }
    toastEl.textContent = msg;
    toastEl.classList.remove("hidden");
  }

  function normalizeActuator(a) {
    if (typeof a === "string") {
      return {
        type: a,
        min_intensity: 0.0,
        max_intensity: 1.0,
        pattern: "constant",
        pulse_hz: 1.0,
      };
    }
    return {
      type: a.type || "Vibrate",
      min_intensity: typeof a.min_intensity === "number" ? a.min_intensity : 0.0,
      max_intensity: typeof a.max_intensity === "number" ? a.max_intensity : 1.0,
      pattern: a.pattern || "constant",
      pulse_hz: typeof a.pulse_hz === "number" ? a.pulse_hz : 1.0,
    };
  }

  function ensureDevicesList(cfg) {
    if (!cfg.buttplug.devices || cfg.buttplug.devices.length === 0) {
      cfg.buttplug.devices = [
        {
          device_index: cfg.buttplug.device_index ?? null,
          device_name: null,
          actuator_types: (cfg.buttplug.actuator_types || ["Vibrate"]).map(normalizeActuator),
          mapping: null,
        },
      ];
    } else {
      cfg.buttplug.devices = cfg.buttplug.devices.map((d) => ({
        device_index: d.device_index ?? null,
        device_name: d.device_name || null,
        actuator_types: (d.actuator_types && d.actuator_types.length ? d.actuator_types : ["Vibrate"]).map(
          normalizeActuator
        ),
        mapping: d.mapping || null,
      }));
    }
  }

  function updateGaugeMarkers() {
    const m = currentConfig.mapping;
    liveModeBadge.textContent = m.mode === "velocity" ? "speed" : "proximity";
    labelActiveWindow.textContent =
      m.mode === "velocity"
        ? `Max Speed: ${Math.round(m.max_speed_mm_s)} mm/s`
        : `Active: ${m.min_range_mm}–${m.max_range_mm}mm (${m.invert ? "closer=stronger" : "further=stronger"})`;
    labelDeadzone.textContent = m.deadzone_mm > 0 ? `Off > ${m.deadzone_mm}mm` : "Deadzone: off";
    labelSmoothing.textContent = `Smoothing: ${Number(m.smoothing).toFixed(2)}`;

    const leftPct = Math.min(100, Math.max(0, (m.min_range_mm / MAX_TRACK_MM) * 100));
    const rightPct = Math.min(100, Math.max(leftPct, (m.max_range_mm / MAX_TRACK_MM) * 100));
    rangeActiveZone.style.left = `${leftPct}%`;
    rangeActiveZone.style.width = `${rightPct - leftPct}%`;

    if (m.deadzone_mm > 0) {
      const dzPct = Math.min(100, Math.max(0, (m.deadzone_mm / MAX_TRACK_MM) * 100));
      rangeDeadzoneLine.style.left = `${dzPct}%`;
      rangeDeadzoneLine.classList.remove("hidden");
    } else {
      rangeDeadzoneLine.classList.add("hidden");
    }
  }

  function syncFormFromConfig() {
    ensureDevicesList(currentConfig);
    const { ble, mapping, buttplug } = currentConfig;

    // Engine mode
    const isStandalone = buttplug.mode === "standalone";
    modeStandaloneBtn.classList.toggle("active", isStandalone);
    modeIntifaceBtn.classList.toggle("active", !isStandalone);
    intifaceAddressRow.classList.toggle("hidden", isStandalone);
    inputServerAddress.value = buttplug.server_address || "ws://127.0.0.1:12345";

    // BLE
    inputBleName.value = ble.device_name || "Fancypants";
    inputDataTimeout.value = ble.data_timeout_secs ?? 5;
    inputSampleMs.value = ble.sample_interval_ms ?? "";
    inputNotifyMs.value = ble.notify_interval_ms ?? "";

    // Mapping
    const isVelocity = mapping.mode === "velocity";
    mapModeDistanceBtn.classList.toggle("active", !isVelocity);
    mapModeVelocityBtn.classList.toggle("active", isVelocity);
    distanceControls.classList.toggle("hidden", isVelocity);
    velocityControls.classList.toggle("hidden", !isVelocity);

    inputInvert.checked = Boolean(mapping.invert);
    inputMinRange.value = mapping.min_range_mm;
    inputMaxRange.value = mapping.max_range_mm;
    inputMaxSpeed.value = mapping.max_speed_mm_s;
    inputDeadzone.value = mapping.deadzone_mm;
    inputSmoothing.value = mapping.smoothing;
    inputMinIntensity.value = mapping.min_intensity;
    inputMaxIntensity.value = mapping.max_intensity;

    valMinRange.textContent = mapping.min_range_mm;
    valMaxRange.textContent = mapping.max_range_mm;
    valMaxSpeed.textContent = Math.round(mapping.max_speed_mm_s);
    valDeadzone.textContent = mapping.deadzone_mm;
    valSmoothing.textContent = Number(mapping.smoothing).toFixed(2);
    valMinIntensity.textContent = `${Math.round(mapping.min_intensity * 100)}%`;
    valMaxIntensity.textContent = `${Math.round(mapping.max_intensity * 100)}%`;

    updateGaugeMarkers();
    renderToysList();
  }

  function renderToysList() {
    toysContainer.innerHTML = "";
    currentConfig.buttplug.devices.forEach((toy, tIdx) => {
      const card = document.createElement("div");
      card.className = "toy-card";

      const hasOverride = toy.mapping !== null && typeof toy.mapping === "object";
      const overrideMode = hasOverride && toy.mapping.mode ? toy.mapping.mode : "inherit";

      card.innerHTML = `
        <div class="toy-card-header">
          <h3>Toy #${tIdx + 1}</h3>
          ${
            currentConfig.buttplug.devices.length > 1
              ? `<button type="button" class="btn btn-danger-ghost" data-remove-toy="${tIdx}">Remove</button>`
              : ""
          }
        </div>
        <div class="grid-2">
          <div class="form-row">
            <label>Match by Name (optional)</label>
            <input type="text" data-toy-name="${tIdx}" placeholder="Any unclaimed toy (e.g. Lush)" value="${
              toy.device_name || ""
            }" />
          </div>
          <div class="form-row">
            <label>Mapping Mode</label>
            <select data-toy-mode="${tIdx}">
              <option value="inherit" ${overrideMode === "inherit" ? "selected" : ""}>Use Default Mapping</option>
              <option value="distance_close" ${
                hasOverride && toy.mapping.mode === "distance" && toy.mapping.invert !== false ? "selected" : ""
              }>Proximity (Closer = Stronger)</option>
              <option value="distance_far" ${
                hasOverride && toy.mapping.mode === "distance" && toy.mapping.invert === false ? "selected" : ""
              }>Proximity (Further = Stronger)</option>
              <option value="velocity" ${overrideMode === "velocity" ? "selected" : ""}>Movement Speed (Velocity)</option>
            </select>
          </div>
        </div>
        <div class="actuator-list" id="actuators-${tIdx}"></div>
        <div>
          <button type="button" class="btn btn-secondary btn-sm" data-add-actuator="${tIdx}">+ Add Actuator</button>
        </div>
      `;

      toysContainer.appendChild(card);

      const actList = card.querySelector(`#actuators-${tIdx}`);
      toy.actuator_types.forEach((act, aIdx) => {
        const norm = normalizeActuator(act);
        const row = document.createElement("div");
        row.className = "actuator-row";
        row.innerHTML = `
          <select data-act-type="${tIdx}:${aIdx}" title="Actuator Type">
            ${ACTUATOR_TYPES.map(
              (t) => `<option value="${t}" ${norm.type === t ? "selected" : ""}>${t}</option>`
            ).join("")}
          </select>
          <select data-act-pattern="${tIdx}:${aIdx}" title="Pattern">
            <option value="constant" ${norm.pattern === "constant" ? "selected" : ""}>Constant</option>
            <option value="pulse" ${norm.pattern === "pulse" ? "selected" : ""}>Pulse (${norm.pulse_hz}Hz)</option>
          </select>
          <input type="number" step="0.1" min="0.1" max="1" data-act-max="${tIdx}:${aIdx}" value="${norm.max_intensity}" title="Max Intensity (0.1 - 1.0)" />
          ${
            toy.actuator_types.length > 1
              ? `<button type="button" class="btn btn-danger-ghost" data-remove-act="${tIdx}:${aIdx}">✕</button>`
              : `<span></span>`
          }
        `;
        actList.appendChild(row);
      });
    });
  }

  async function pushLiveConfig() {
    // Keep top-level actuator_types synced with the first device for single-toy backward compatibility
    if (currentConfig.buttplug.devices.length > 0) {
      currentConfig.buttplug.actuator_types = currentConfig.buttplug.devices[0].actuator_types;
    }
    updateGaugeMarkers();
    try {
      await invoke("update_live_config", { config: currentConfig });
      saveStatus.textContent = "Applied live";
      showToast(null);
    } catch (e) {
      showToast(String(e));
    }
  }

  // Event Listeners for Controls
  modeStandaloneBtn.addEventListener("click", () => {
    currentConfig.buttplug.mode = "standalone";
    syncFormFromConfig();
    pushLiveConfig();
  });

  modeIntifaceBtn.addEventListener("click", () => {
    currentConfig.buttplug.mode = "intiface";
    syncFormFromConfig();
    pushLiveConfig();
  });

  inputServerAddress.addEventListener("change", () => {
    currentConfig.buttplug.server_address = inputServerAddress.value.trim() || "ws://127.0.0.1:12345";
    pushLiveConfig();
  });

  inputBleName.addEventListener("change", () => {
    currentConfig.ble.device_name = inputBleName.value.trim() || "Fancypants";
    pushLiveConfig();
  });

  inputDataTimeout.addEventListener("change", () => {
    currentConfig.ble.data_timeout_secs = Math.max(0, parseInt(inputDataTimeout.value || "5", 10));
    pushLiveConfig();
  });

  inputSampleMs.addEventListener("change", () => {
    const v = parseInt(inputSampleMs.value, 10);
    currentConfig.ble.sample_interval_ms = Number.isFinite(v) ? Math.min(5000, Math.max(10, v)) : null;
    pushLiveConfig();
  });

  inputNotifyMs.addEventListener("change", () => {
    const v = parseInt(inputNotifyMs.value, 10);
    currentConfig.ble.notify_interval_ms = Number.isFinite(v) ? Math.min(5000, Math.max(10, v)) : null;
    pushLiveConfig();
  });

  mapModeDistanceBtn.addEventListener("click", () => {
    currentConfig.mapping.mode = "distance";
    syncFormFromConfig();
    pushLiveConfig();
  });

  mapModeVelocityBtn.addEventListener("click", () => {
    currentConfig.mapping.mode = "velocity";
    syncFormFromConfig();
    pushLiveConfig();
  });

  inputInvert.addEventListener("change", () => {
    currentConfig.mapping.invert = inputInvert.checked;
    pushLiveConfig();
  });

  inputMinRange.addEventListener("input", () => {
    let minVal = parseInt(inputMinRange.value, 10);
    if (minVal >= currentConfig.mapping.max_range_mm) {
      minVal = Math.max(10, currentConfig.mapping.max_range_mm - 10);
      inputMinRange.value = minVal;
    }
    currentConfig.mapping.min_range_mm = minVal;
    valMinRange.textContent = minVal;
    pushLiveConfig();
  });

  inputMaxRange.addEventListener("input", () => {
    let maxVal = parseInt(inputMaxRange.value, 10);
    if (maxVal <= currentConfig.mapping.min_range_mm) {
      maxVal = currentConfig.mapping.min_range_mm + 10;
      inputMaxRange.value = maxVal;
    }
    currentConfig.mapping.max_range_mm = maxVal;
    valMaxRange.textContent = maxVal;
    pushLiveConfig();
  });

  inputMaxSpeed.addEventListener("input", () => {
    const v = parseFloat(inputMaxSpeed.value);
    currentConfig.mapping.max_speed_mm_s = v;
    valMaxSpeed.textContent = Math.round(v);
    pushLiveConfig();
  });

  inputDeadzone.addEventListener("input", () => {
    const v = parseInt(inputDeadzone.value, 10);
    currentConfig.mapping.deadzone_mm = v;
    valDeadzone.textContent = v;
    pushLiveConfig();
  });

  inputSmoothing.addEventListener("input", () => {
    const v = parseFloat(inputSmoothing.value);
    currentConfig.mapping.smoothing = v;
    valSmoothing.textContent = v.toFixed(2);
    pushLiveConfig();
  });

  inputMinIntensity.addEventListener("input", () => {
    let v = parseFloat(inputMinIntensity.value);
    if (v > currentConfig.mapping.max_intensity) {
      v = currentConfig.mapping.max_intensity;
      inputMinIntensity.value = v;
    }
    currentConfig.mapping.min_intensity = v;
    valMinIntensity.textContent = `${Math.round(v * 100)}%`;
    pushLiveConfig();
  });

  inputMaxIntensity.addEventListener("input", () => {
    let v = parseFloat(inputMaxIntensity.value);
    if (v < currentConfig.mapping.min_intensity) {
      v = currentConfig.mapping.min_intensity;
      inputMaxIntensity.value = v;
    }
    currentConfig.mapping.max_intensity = v;
    valMaxIntensity.textContent = `${Math.round(v * 100)}%`;
    pushLiveConfig();
  });

  // Delegated events for dynamic toy list
  toysContainer.addEventListener("input", (e) => {
    const target = e.target;
    if (target.dataset.toyName !== undefined) {
      const idx = parseInt(target.dataset.toyName, 10);
      const val = target.value.trim();
      currentConfig.buttplug.devices[idx].device_name = val.length > 0 ? val : null;
      pushLiveConfig();
    } else if (target.dataset.actMax !== undefined) {
      const [tIdx, aIdx] = target.dataset.actMax.split(":").map(Number);
      const v = Math.min(1.0, Math.max(0.05, parseFloat(target.value) || 1.0));
      currentConfig.buttplug.devices[tIdx].actuator_types[aIdx].max_intensity = v;
      pushLiveConfig();
    }
  });

  toysContainer.addEventListener("change", (e) => {
    const target = e.target;
    if (target.dataset.toyMode !== undefined) {
      const idx = parseInt(target.dataset.toyMode, 10);
      const val = target.value;
      if (val === "inherit") {
        currentConfig.buttplug.devices[idx].mapping = null;
      } else if (val === "distance_close") {
        currentConfig.buttplug.devices[idx].mapping = { mode: "distance", invert: true };
      } else if (val === "distance_far") {
        currentConfig.buttplug.devices[idx].mapping = { mode: "distance", invert: false };
      } else if (val === "velocity") {
        currentConfig.buttplug.devices[idx].mapping = { mode: "velocity" };
      }
      pushLiveConfig();
    } else if (target.dataset.actType !== undefined) {
      const [tIdx, aIdx] = target.dataset.actType.split(":").map(Number);
      currentConfig.buttplug.devices[tIdx].actuator_types[aIdx].type = target.value;
      pushLiveConfig();
    } else if (target.dataset.actPattern !== undefined) {
      const [tIdx, aIdx] = target.dataset.actPattern.split(":").map(Number);
      currentConfig.buttplug.devices[tIdx].actuator_types[aIdx].pattern = target.value;
      currentConfig.buttplug.devices[tIdx].actuator_types[aIdx].pulse_hz = 2.0;
      renderToysList();
      pushLiveConfig();
    }
  });

  toysContainer.addEventListener("click", (e) => {
    const target = e.target;
    if (target.dataset.removeToy !== undefined) {
      const idx = parseInt(target.dataset.removeToy, 10);
      currentConfig.buttplug.devices.splice(idx, 1);
      renderToysList();
      pushLiveConfig();
    } else if (target.dataset.addActuator !== undefined) {
      const idx = parseInt(target.dataset.addActuator, 10);
      currentConfig.buttplug.devices[idx].actuator_types.push({
        type: "Vibrate",
        min_intensity: 0.0,
        max_intensity: 1.0,
        pattern: "constant",
        pulse_hz: 2.0,
      });
      renderToysList();
      pushLiveConfig();
    } else if (target.dataset.removeAct !== undefined) {
      const [tIdx, aIdx] = target.dataset.removeAct.split(":").map(Number);
      currentConfig.buttplug.devices[tIdx].actuator_types.splice(aIdx, 1);
      renderToysList();
      pushLiveConfig();
    }
  });

  btnAddToy.addEventListener("click", () => {
    currentConfig.buttplug.devices.push({
      device_index: null,
      device_name: null,
      actuator_types: [
        { type: "Vibrate", min_intensity: 0.0, max_intensity: 1.0, pattern: "constant", pulse_hz: 1.0 },
      ],
      mapping: null,
    });
    renderToysList();
    pushLiveConfig();
  });

  btnSaveConfig.addEventListener("click", async () => {
    try {
      await invoke("save_config", { config: currentConfig });
      saveStatus.textContent = "Saved to config.toml ✓";
      showToast(null);
    } catch (e) {
      showToast(String(e));
    }
  });

  btnSession.addEventListener("click", async () => {
    try {
      if (sessionRunning) {
        await invoke("stop_session");
      } else {
        await invoke("start_session");
      }
    } catch (e) {
      showToast(String(e));
    }
  });

  function renderTelemetry(t) {
    if (!t) return;
    sessionRunning = t.state !== "stopped";
    btnSession.textContent = sessionRunning ? "Stop" : "Start";
    btnSession.classList.toggle("stop-mode", sessionRunning);

    statusText.textContent = t.message || t.state;
    statusDot.className = "status-dot";
    if (t.state === "running") {
      statusDot.classList.add("running");
    } else if (t.state === "scanning_sensor" || t.state === "connecting_toys" || t.state === "reconnecting") {
      statusDot.classList.add("connecting");
    } else if (t.state === "error") {
      statusDot.classList.add("error");
    }

    if (typeof t.battery_pct === "number") {
      batteryValue.textContent = `${t.battery_pct}%`;
      batteryPill.classList.toggle("low-battery", t.battery_pct <= 20);
    }

    if (typeof t.distance_mm === "number") {
      metricDistance.textContent = t.distance_mm;
      const pct = Math.min(100, Math.max(0, (t.distance_mm / MAX_TRACK_MM) * 100));
      rangeCursor.style.left = `${pct}%`;
      rangeCursor.classList.remove("hidden");
    } else {
      metricDistance.textContent = "---";
      rangeCursor.classList.add("hidden");
    }

    const intensityPct = Math.round((t.mapped_intensity || 0) * 100);
    metricIntensity.textContent = intensityPct;
    intensityFill.style.width = `${intensityPct}%`;

    // Render live toys
    liveToysList.innerHTML = "";
    if (Array.isArray(t.toys)) {
      t.toys.forEach((toy) => {
        const toyPct = Math.round((toy.intensity || 0) * 100);
        const item = document.createElement("div");
        item.className = "live-toy-item";
        item.innerHTML = `
          <span><strong>${toy.name}</strong> <small>(${toy.mode})</small></span>
          <div class="live-toy-bar"><div class="live-toy-bar-fill" style="width: ${toyPct}%"></div></div>
          <span>${toyPct}%</span>
        `;
        liveToysList.appendChild(item);
      });
    }
  }

  // Initialize
  (async () => {
    try {
      const cfg = await invoke("get_config");
      if (cfg) {
        currentConfig = cfg;
      }
      syncFormFromConfig();
      const initialTelemetry = await invoke("get_telemetry");
      if (initialTelemetry) {
        renderTelemetry(initialTelemetry);
      }
      await listen("telemetry", (event) => {
        renderTelemetry(event.payload);
      });
    } catch (e) {
      console.error("Initialization error:", e);
    }
  })();
})();
