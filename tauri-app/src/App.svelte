<script lang="ts">
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { open as openShell } from "@tauri-apps/plugin-shell";
  import { open as openDialog } from "@tauri-apps/plugin-dialog";

  type ServerInfo = { id: number; project: string };
  type Config = {
    arizona: { login: string; password: string };
    rodina: { login: string; password: string };
    servers: Record<string, { 
      forum_uk_url: string; 
      forum_uk_url_2: string;
      forum_pdd_url: string; 
      forum_pdd_url_2: string;
    }>;
    output_dir: string;
    ai: {
      provider: string;
      openai_api_key: string;
      openai_model: string;
      gemini_api_key: string;
      gemini_model: string;
    };
  };

  type ProgressPayload = { step: string; percent: number };

  let configPath = "";
  let servers: ServerInfo[] = [];
  let cfg: Config = {
    arizona: { login: "", password: "" },
    rodina: { login: "", password: "" },
    servers: {},
    output_dir: "",
    ai: {
      provider: "gemini",
      openai_api_key: "",
      openai_model: "gpt-4.1-mini",
      gemini_api_key: "",
      gemini_model: "gemini-2.0-flash"
    }
  };
  
  let logText = "";
  let selectedServers = new Set<number>();
  let mode: "uk" | "pdd" | "both" = "both";
  let progress = 0;
  let progressStep = "idle";
  let skipLogin = false;
  let activeTab: "config" | "run" | "servers" = "config";
  let showServerPopup = false;

  // Константы серверов (по старому приложению)
  const ARIZONA_PC = Array.from({ length: 32 }, (_, i) => i + 1);
  const ARIZONA_MOBILE = [101, 102, 103];
  const ARIZONA_VC = [200];
  const RODINA_PC = Array.from({ length: 7 }, (_, i) => i + 301);
  const RODINA_MOBILE = [401, 402];

  const ALL_SERVERS = [...ARIZONA_PC, ...ARIZONA_MOBILE, ...ARIZONA_VC, ...RODINA_PC, ...RODINA_MOBILE];

  const appendLog = (line: string) => {
    logText += line + "\n";
  };

  const loadAll = async () => {
    cfg = (await invoke("load_config")) as Config;
    servers = (await invoke("get_servers")) as ServerInfo[];
    configPath = (await invoke("get_config_path")) as string;
  };

  const save = async () => {
    await invoke("save_config", { config: cfg });
    appendLog("✓ Saved config.json");
  };

  const reload = async () => {
    await loadAll();
    appendLog("✓ Reloaded config.json");
  };

  const openConfig = async () => {
    if (!configPath) return;
    await openShell(configPath);
  };

  const pickOutputDir = async () => {
    const result = await openDialog({ directory: true, multiple: false, title: "Select output folder" });
    if (typeof result === "string") {
      cfg.output_dir = result;
      appendLog(`Output dir: ${result}`);
    }
  };

  const toggleServer = (id: number) => {
    if (selectedServers.has(id)) {
      selectedServers.delete(id);
    } else {
      selectedServers.add(id);
    }
    selectedServers = selectedServers;
  };

  const selectGroup = (group: number[]) => {
    selectedServers = new Set(group);
  };

  const clearServers = () => {
    selectedServers = new Set();
  };

  const runUpdate = async () => {
    if (selectedServers.size === 0) {
      appendLog("No servers selected");
      return;
    }

    progress = 0;
    progressStep = "starting";
    appendLog(`Update start: ${selectedServers.size} server(s), mode=${mode}`);
    
    for (const serverId of Array.from(selectedServers).sort((a, b) => a - b)) {
      try {
        await invoke("run_update", {
          serverNum: serverId,
          mode,
          skipLogin
        });
        appendLog(`✓ Server ${serverId} done`);
      } catch (e) {
        appendLog(`✗ Server ${serverId}: ${e}`);
      }
    }
  };

  const getSelectedLabel = () => {
    if (selectedServers.size === 0) return "🖥  No servers  ▾";
    if (selectedServers.size === 1) return `🖥  Server ${Array.from(selectedServers)[0]}  ▾`;
    return `🖥  ${selectedServers.size} servers  ▾`;
  };

  onMount(async () => {
    await loadAll();

    const unlistenLog = await listen<string>("log", (event) => {
      appendLog(event.payload);
    });

    const unlistenProgress = await listen<ProgressPayload>("progress", (event) => {
      progressStep = event.payload.step;
      progress = event.payload.percent;
    });

    return () => {
      unlistenLog();
      unlistenProgress();
    };
  });
</script>

<div class="shell">
  <header class="hero">
    <div>
      <h1>⚙️ Smart Config Editor</h1>
      <p>AI-powered SmartUK/SmartPDD updates with smart forum parsing.</p>
    </div>
    <div class="hero-meta">
      <span class="chip">Config: {configPath.split("/").pop() || "—"}</span>
      <span class="chip">Mode: <strong>{mode.toUpperCase()}</strong></span>
    </div>
  </header>

  <nav class="tabs">
    <button class:active={activeTab === "config"} on:click={() => (activeTab = "config")}>⚙️  Settings</button>
    <button class:active={activeTab === "run"} on:click={() => (activeTab = "run")}>▶️  Run</button>
    <button class:active={activeTab === "servers"} on:click={() => (activeTab = "servers")}>🖥️  Servers</button>
  </nav>

  <div class="page">
    {#if activeTab === "config"}
      <div class="panel">
        <h2>Login Credentials</h2>
        <div class="grid-2">
          <div>
            <label for="az-login">Arizona login</label>
            <input id="az-login" bind:value={cfg.arizona.login} placeholder="admin" />
          </div>
          <div>
            <label for="az-pass">Arizona password</label>
            <input id="az-pass" type="password" bind:value={cfg.arizona.password} />
          </div>
          <div>
            <label for="rd-login">Rodina login</label>
            <input id="rd-login" bind:value={cfg.rodina.login} placeholder="admin" />
          </div>
          <div>
            <label for="rd-pass">Rodina password</label>
            <input id="rd-pass" type="password" bind:value={cfg.rodina.password} />
          </div>
        </div>
      </div>

      <div class="panel">
        <h2>AI Settings</h2>
        <div class="grid-2">
          <div>
            <label for="provider">Provider</label>
            <select id="provider" bind:value={cfg.ai.provider}>
              <option value="openai">OpenAI</option>
              <option value="gemini">Gemini</option>
            </select>
          </div>
          <div>
            <label for="output-dir">Output dir</label>
            <div class="input-group">
              <input id="output-dir" bind:value={cfg.output_dir} placeholder="app folder" readonly />
              <button class="btn-icon" on:click={pickOutputDir} title="Browse">📂</button>
            </div>
          </div>
        </div>

        {#if cfg.ai.provider === "openai"}
          <div class="grid-2">
            <div>
              <label for="openai-key">OpenAI API key</label>
              <input id="openai-key" type="password" bind:value={cfg.ai.openai_api_key} />
            </div>
            <div>
              <label for="openai-model">OpenAI model</label>
              <input id="openai-model" bind:value={cfg.ai.openai_model} />
            </div>
          </div>
        {:else}
          <div class="grid-2">
            <div>
              <label for="gemini-key">Gemini API key</label>
              <input id="gemini-key" type="password" bind:value={cfg.ai.gemini_api_key} />
            </div>
            <div>
              <label for="gemini-model">Gemini model</label>
              <input id="gemini-model" bind:value={cfg.ai.gemini_model} />
            </div>
          </div>
        {/if}
      </div>

      <div class="panel-actions">
        <button class="btn-primary" on:click={save}>💾  Save</button>
        <button class="btn-secondary" on:click={reload}>🔄  Reload</button>
        <button class="btn-ghost" on:click={openConfig}>📄  Open config.json</button>
      </div>
    {/if}

    {#if activeTab === "run"}
      <div class="panel">
        <h2>Server Selector</h2>
        <div class="server-selector">
          <button class="server-btn" on:click={() => (showServerPopup = !showServerPopup)}>
            {getSelectedLabel()}
          </button>
          <button class="btn-clear" on:click={clearServers} title="Clear selection">✕</button>
        </div>

        {#if showServerPopup}
          <div class="server-popup">
            <div class="popup-row">
              <button class="quick-btn" on:click={() => selectGroup([...ARIZONA_PC, ...ARIZONA_MOBILE, ...ARIZONA_VC])}>Arizona</button>
              <button class="quick-btn" on:click={() => selectGroup(ARIZONA_MOBILE)}>Mobile</button>
              <button class="quick-btn" on:click={() => selectGroup([...RODINA_PC, ...RODINA_MOBILE])}>Rodina</button>
              <button class="quick-btn" on:click={() => selectGroup(ALL_SERVERS)}>All</button>
            </div>
            
            <div class="popup-divider"></div>

            <div class="popup-content">
              <div class="server-column">
                <h4>Arizona PC (1–32)</h4>
                {#each ARIZONA_PC as id}
                  <label class="checkbox">
                    <input type="checkbox" checked={selectedServers.has(id)} on:change={() => toggleServer(id)} />
                    <span>{id}</span>
                  </label>
                {/each}
              </div>

              <div class="server-column">
                <h4>Arizona Mobile & VC</h4>
                {#each [...ARIZONA_MOBILE, ...ARIZONA_VC] as id}
                  <label class="checkbox">
                    <input type="checkbox" checked={selectedServers.has(id)} on:change={() => toggleServer(id)} />
                    <span>{id}</span>
                  </label>
                {/each}
              </div>

              <div class="server-column">
                <h4>Rodina PC & Mobile</h4>
                {#each [...RODINA_PC, ...RODINA_MOBILE] as id}
                  <label class="checkbox">
                    <input type="checkbox" checked={selectedServers.has(id)} on:change={() => toggleServer(id)} />
                    <span>{id}</span>
                  </label>
                {/each}
              </div>
            </div>
          </div>
        {/if}
      </div>

      <div class="panel">
        <h2>Update Settings</h2>
        <div class="grid-2">
          <div>
            <label for="mode">Mode</label>
            <select id="mode" bind:value={mode}>
              <option value="uk">UK only</option>
              <option value="pdd">PDD only</option>
              <option value="both">Both UK & PDD</option>
            </select>
          </div>
          <label class="checkbox-label" for="skip-login">
            <input id="skip-login" type="checkbox" bind:checked={skipLogin} />
            <span>Skip login (if not required)</span>
          </label>
        </div>
      </div>

      <div class="panel-actions">
        <button class="btn-primary" on:click={runUpdate}>▶️  RUN UPDATE</button>
      </div>

      <div class="panel">
        <h2>Progress</h2>
        <div class="progress-box">
          <div class="progress-label">{progressStep}</div>
          <div class="progress-bar">
            <div class="progress-fill" style={`width: ${progress}%`}></div>
          </div>
          <div class="progress-percent">{progress}%</div>
        </div>
      </div>

      <div class="panel log-panel">
        <h2>Log</h2>
        <div class="log">{logText}</div>
      </div>
    {/if}

    {#if activeTab === "servers"}
      <div class="panel">
        <h2>Server URLs (Arizona PC 1–8)</h2>
        {#each ARIZONA_PC.slice(0, 8) as id}
          <div class="server-card">
            <div class="card-header">Server {id}</div>
            <div class="grid-2">
              <div>
                <label for={`uk-${id}-1`}>UK URL #1</label>
                <input id={`uk-${id}-1`} bind:value={cfg.servers[id.toString()].forum_uk_url} placeholder="https://..." />
              </div>
              <div>
                <label for={`uk-${id}-2`}>UK URL #2</label>
                <input id={`uk-${id}-2`} bind:value={cfg.servers[id.toString()].forum_uk_url_2} placeholder="https://..." />
              </div>
              <div>
                <label for={`pdd-${id}-1`}>PDD URL #1</label>
                <input id={`pdd-${id}-1`} bind:value={cfg.servers[id.toString()].forum_pdd_url} placeholder="https://..." />
              </div>
              <div>
                <label for={`pdd-${id}-2`}>PDD URL #2</label>
                <input id={`pdd-${id}-2`} bind:value={cfg.servers[id.toString()].forum_pdd_url_2} placeholder="https://..." />
              </div>
            </div>
          </div>
        {/each}
      </div>
    {/if}
  </div>
</div>

<style>
  :global(body) {
    margin: 0;
    padding: 0;
    font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif;
    background: linear-gradient(135deg, #0f1419 0%, #1a1f2e 100%);
    color: #e6eaf2;
  }

  .shell {
    display: flex;
    flex-direction: column;
    height: 100vh;
    background: #141826;
    overflow: hidden;
  }

  .hero {
    padding: 20px 24px;
    background: linear-gradient(135deg, rgba(91, 124, 255, 0.15) 0%, rgba(138, 43, 226, 0.1) 100%);
    border-bottom: 1px solid rgba(120, 130, 160, 0.2);
    display: flex;
    justify-content: space-between;
    align-items: center;
  }

  .hero h1 {
    margin: 0;
    font-size: 24px;
    font-weight: 600;
    color: #fff;
  }

  .hero p {
    margin: 4px 0 0 0;
    font-size: 13px;
    color: #aab4cc;
  }

  .hero-meta {
    display: flex;
    gap: 12px;
  }

  .chip {
    background: rgba(255, 255, 255, 0.08);
    border: 1px solid rgba(120, 130, 160, 0.2);
    border-radius: 6px;
    padding: 6px 12px;
    font-size: 12px;
    color: #aab4cc;
  }

  .chip strong {
    color: #5b7cff;
    font-weight: 600;
  }

  nav.tabs {
    display: flex;
    gap: 8px;
    padding: 12px 24px;
    background: rgba(0, 0, 0, 0.2);
    border-bottom: 1px solid rgba(120, 130, 160, 0.1);
    overflow-x: auto;
  }

  nav.tabs button {
    padding: 8px 16px;
    border: none;
    background: transparent;
    color: #aab4cc;
    border-radius: 6px;
    cursor: pointer;
    transition: all 0.2s;
    white-space: nowrap;
    font-size: 14px;
  }

  nav.tabs button:hover {
    background: rgba(91, 124, 255, 0.1);
    color: #e6eaf2;
  }

  nav.tabs button.active {
    background: #5b7cff;
    color: #fff;
  }

  .page {
    flex: 1;
    overflow-y: auto;
    padding: 20px 24px;
    display: flex;
    flex-direction: column;
    gap: 16px;
  }

  .panel {
    background: rgba(255, 255, 255, 0.04);
    border: 1px solid rgba(120, 130, 160, 0.15);
    border-radius: 12px;
    padding: 16px;
  }

  .panel h2 {
    margin: 0 0 12px 0;
    font-size: 16px;
    font-weight: 600;
    color: #fff;
  }

  .grid-2 {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(200px, 1fr));
    gap: 12px;
  }

  .grid-2 > div {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  label {
    font-size: 12px;
    font-weight: 500;
    color: #aab4cc;
    text-transform: uppercase;
    letter-spacing: 0.5px;
  }

  input[type="text"],
  input[type="password"],
  select {
    background: rgba(255, 255, 255, 0.06);
    border: 1px solid rgba(120, 130, 160, 0.2);
    color: #e6eaf2;
    padding: 8px 12px;
    border-radius: 6px;
    font-size: 13px;
    transition: all 0.2s;
  }

  input:focus,
  select:focus {
    outline: none;
    border-color: #5b7cff;
    background: rgba(255, 255, 255, 0.08);
    box-shadow: 0 0 0 3px rgba(91, 124, 255, 0.1);
  }

  input:readonly {
    background: rgba(255, 255, 255, 0.02);
    cursor: not-allowed;
  }

  .input-group {
    display: flex;
    gap: 6px;
  }

  .input-group input {
    flex: 1;
  }

  .btn-icon {
    padding: 8px 12px;
    background: rgba(91, 124, 255, 0.2);
    border: 1px solid rgba(91, 124, 255, 0.3);
    color: #5b7cff;
    border-radius: 6px;
    cursor: pointer;
    transition: all 0.2s;
  }

  .btn-icon:hover {
    background: rgba(91, 124, 255, 0.3);
  }

  .panel-actions {
    display: flex;
    gap: 8px;
    justify-content: center;
  }

  .btn-primary,
  .btn-secondary,
  .btn-ghost {
    padding: 10px 20px;
    border: none;
    border-radius: 8px;
    cursor: pointer;
    font-weight: 600;
    font-size: 14px;
    transition: all 0.2s;
  }

  .btn-primary {
    background: linear-gradient(135deg, #5b7cff 0%, #8a2be2 100%);
    color: #fff;
  }

  .btn-primary:hover {
    transform: translateY(-2px);
    box-shadow: 0 8px 16px rgba(91, 124, 255, 0.3);
  }

  .btn-secondary {
    background: rgba(255, 255, 255, 0.1);
    color: #aab4cc;
  }

  .btn-secondary:hover {
    background: rgba(255, 255, 255, 0.15);
    color: #e6eaf2;
  }

  .btn-ghost {
    background: transparent;
    color: #aab4cc;
    border: 1px solid rgba(120, 130, 160, 0.2);
  }

  .btn-ghost:hover {
    color: #e6eaf2;
    border-color: rgba(120, 130, 160, 0.4);
  }

  .server-selector {
    display: flex;
    gap: 8px;
    margin-bottom: 12px;
  }

  .server-btn {
    flex: 1;
    padding: 10px 16px;
    background: rgba(91, 124, 255, 0.15);
    border: 1px solid rgba(91, 124, 255, 0.3);
    color: #5b7cff;
    border-radius: 8px;
    cursor: pointer;
    font-weight: 600;
    transition: all 0.2s;
  }

  .server-btn:hover {
    background: rgba(91, 124, 255, 0.25);
  }

  .btn-clear {
    padding: 10px 16px;
    background: rgba(255, 100, 100, 0.2);
    border: 1px solid rgba(255, 100, 100, 0.3);
    color: #ff6464;
    border-radius: 8px;
    cursor: pointer;
    font-weight: 600;
    transition: all 0.2s;
  }

  .btn-clear:hover {
    background: rgba(255, 100, 100, 0.3);
  }

  .server-popup {
    background: rgba(0, 0, 0, 0.3);
    border: 1px solid rgba(120, 130, 160, 0.2);
    border-radius: 8px;
    padding: 12px;
    margin-top: 8px;
  }

  .popup-row {
    display: grid;
    grid-template-columns: repeat(4, 1fr);
    gap: 8px;
    margin-bottom: 12px;
  }

  .quick-btn {
    padding: 8px 12px;
    background: rgba(91, 124, 255, 0.15);
    border: 1px solid rgba(91, 124, 255, 0.3);
    color: #5b7cff;
    border-radius: 6px;
    cursor: pointer;
    font-size: 12px;
    font-weight: 600;
    transition: all 0.2s;
  }

  .quick-btn:hover {
    background: rgba(91, 124, 255, 0.25);
  }

  .popup-divider {
    height: 1px;
    background: rgba(120, 130, 160, 0.2);
    margin: 8px 0;
  }

  .popup-content {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 16px;
    max-height: 400px;
    overflow-y: auto;
  }

  .server-column h4 {
    margin: 0 0 8px 0;
    font-size: 12px;
    font-weight: 600;
    color: #5b7cff;
    text-transform: uppercase;
  }

  .checkbox {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 4px;
    cursor: pointer;
    font-size: 13px;
    user-select: none;
  }

  .checkbox input[type="checkbox"] {
    width: 16px;
    height: 16px;
    cursor: pointer;
    accent-color: #5b7cff;
  }

  .checkbox-label {
    display: flex;
    align-items: center;
    gap: 8px;
    cursor: pointer;
    font-weight: 400;
    text-transform: none;
  }

  .checkbox-label input {
    width: 16px;
    height: 16px;
    cursor: pointer;
    accent-color: #5b7cff;
  }

  .progress-box {
    display: grid;
    grid-template-columns: 100px 1fr 60px;
    gap: 12px;
    align-items: center;
  }

  .progress-label {
    font-size: 12px;
    font-weight: 600;
    color: #aab4cc;
    text-transform: uppercase;
  }

  .progress-bar {
    background: rgba(0, 0, 0, 0.3);
    border-radius: 4px;
    height: 8px;
    overflow: hidden;
  }

  .progress-fill {
    background: linear-gradient(90deg, #5b7cff 0%, #8a2be2 100%);
    height: 100%;
    transition: width 0.3s;
  }

  .progress-percent {
    font-size: 12px;
    font-weight: 600;
    color: #5b7cff;
    text-align: right;
  }

  .log-panel {
    flex: 1;
    display: flex;
    flex-direction: column;
  }

  .log {
    flex: 1;
    background: rgba(0, 0, 0, 0.4);
    border: 1px solid rgba(120, 130, 160, 0.15);
    border-radius: 6px;
    padding: 10px;
    font-family: "Monaco", "Courier New", monospace;
    font-size: 11px;
    color: #7a8a9e;
    line-height: 1.5;
    overflow-y: auto;
    white-space: pre-wrap;
    word-wrap: break-word;
  }

  .server-card {
    background: rgba(0, 0, 0, 0.2);
    border: 1px solid rgba(120, 130, 160, 0.1);
    border-radius: 8px;
    padding: 12px;
    margin-bottom: 12px;
  }

  .card-header {
    font-size: 13px;
    font-weight: 600;
    color: #5b7cff;
    margin-bottom: 12px;
  }

  @media (max-width: 1024px) {
    .popup-content {
      grid-template-columns: repeat(2, 1fr);
    }
  }

  @media (max-width: 768px) {
    .popup-content {
      grid-template-columns: 1fr;
    }

    .grid-2 {
      grid-template-columns: 1fr;
    }
  }
</style>
