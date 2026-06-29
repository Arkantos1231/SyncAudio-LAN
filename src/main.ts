import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

interface AppConfig {
  role: "sender" | "receiver";
  last_server_ip: string;
  auto_connect: boolean;
  buffer_size_ms: number;
}

// ── DOM refs ──────────────────────────────────────────────────────────────────
const dot        = document.getElementById("status-dot")!;
const statusText = document.getElementById("status-text")!;
const roleSelect = document.getElementById("role-select") as HTMLSelectElement;
const ipInput    = document.getElementById("ip-input") as HTMLInputElement;
const ipField    = document.getElementById("ip-field")!;
const roleHint   = document.getElementById("role-hint")!;
const bufSlider  = document.getElementById("buffer-slider") as HTMLInputElement;
const bufLabel   = document.getElementById("buffer-label")!;
const bufValue   = document.getElementById("buffer-value")!;
const autoCheck  = document.getElementById("auto-connect-check") as HTMLInputElement;
const connectBtn = document.getElementById("connect-btn") as HTMLButtonElement;
const discBtn    = document.getElementById("disconnect-btn") as HTMLButtonElement;
const localIpEl  = document.getElementById("local-ip")!;
const errorBox   = document.getElementById("error-box")!;

// ── Status UI ─────────────────────────────────────────────────────────────────
const STATUS_LABELS: Record<string, string> = {
  idle:       "Desconectado",
  connecting: "Conectando…",
  connected:  "Conectado",
  error:      "Error de conexión",
};

function setStatus(status: string) {
  dot.className = `dot ${status}`;
  statusText.textContent = STATUS_LABELS[status] ?? status;
  const busy = status === "connecting";
  const live  = status === "connected";
  connectBtn.disabled    = live || busy;
  discBtn.disabled       = !live && !busy;
  if (status !== "error") hideError();
}

function showError(msg: string) {
  errorBox.textContent = `⚠ ${msg}`;
  errorBox.style.display = "block";
}
function hideError() {
  errorBox.style.display = "none";
}

// ── Role UI ───────────────────────────────────────────────────────────────────
const ROLE_HINTS: Record<string, string> = {
  sender:   "Captura el audio del sistema y lo envía a la IP del Receptor.",
  receiver: "Escucha en el puerto 44444 y reproduce el audio en los parlantes locales.",
};

function updateRoleUI() {
  const role = roleSelect.value;
  ipField.style.display = role === "sender" ? "" : "none";
  roleHint.textContent  = ROLE_HINTS[role] ?? "";
}

// ── Collect current form values ───────────────────────────────────────────────
function collectConfig(): AppConfig {
  return {
    role:            roleSelect.value as "sender" | "receiver",
    last_server_ip:  ipInput.value.trim(),
    auto_connect:    autoCheck.checked,
    buffer_size_ms:  Number(bufSlider.value),
  };
}

// ── Init ──────────────────────────────────────────────────────────────────────
async function init() {
  // Load saved config from backend
  const cfg: AppConfig = await invoke("get_config");
  roleSelect.value    = cfg.role;
  ipInput.value       = cfg.last_server_ip;
  bufSlider.value     = String(cfg.buffer_size_ms);
  autoCheck.checked   = cfg.auto_connect;
  bufLabel.textContent = String(cfg.buffer_size_ms);
  bufValue.textContent = String(cfg.buffer_size_ms);
  updateRoleUI();

  // Detect local IP for display
  invoke<string>("get_local_ip")
    .then(ip => { localIpEl.textContent = ip; })
    .catch(() => { localIpEl.textContent = "—"; });

  // Sync initial status from backend (in case auto-connect already fired)
  const statusCode: number = await invoke("get_status");
  const codeMap: Record<number, string> = { 0: "idle", 1: "connecting", 2: "connected", 3: "error" };
  setStatus(codeMap[statusCode] ?? "idle");

  // Listen for real-time status events from Rust
  await listen<string>("status-changed", e => setStatus(e.payload));
  await listen<string>("error-message",  e => showError(e.payload));
}

// ── Event listeners ───────────────────────────────────────────────────────────
roleSelect.addEventListener("change", updateRoleUI);

bufSlider.addEventListener("input", () => {
  bufLabel.textContent = bufSlider.value;
  bufValue.textContent = bufSlider.value;
});

connectBtn.addEventListener("click", async () => {
  hideError();
  const cfg = collectConfig();
  try {
    await invoke("set_config", { config: cfg });
    // Update autostart registry entry
    await invoke("set_autostart", { enabled: cfg.auto_connect });
    await invoke("connect");
  } catch (e) {
    showError(String(e));
    setStatus("error");
  }
});

discBtn.addEventListener("click", async () => {
  try {
    await invoke("disconnect");
  } catch (e) {
    showError(String(e));
  }
});

// Persist config changes on blur (auto_connect checkbox)
autoCheck.addEventListener("change", async () => {
  try {
    const cfg = collectConfig();
    await invoke("set_config", { config: cfg });
    await invoke("set_autostart", { enabled: cfg.auto_connect });
  } catch (_) {}
});

init().catch(console.error);
