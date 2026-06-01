<script lang="ts">
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { open as openShell } from "@tauri-apps/plugin-shell";
  import { open as openDialog } from "@tauri-apps/plugin-dialog";

  type ServerLinks = {
    forum_uk_url: string;
    forum_uk_url_2: string;
    forum_pdd_url: string;
    forum_pdd_url_2: string;
  };

  type Config = {
    arizona: { login: string; password: string };
    rodina: { login: string; password: string };
    servers: Record<string, ServerLinks>;
    output_dir: string;
    ai: {
      provider: string;
      openai_api_keys: string[];
      openai_api_key: string;
      openai_model: string;
      gemini_api_keys: string[];
      gemini_api_key: string;
      gemini_model: string;
    };
  };

  const emptyConfig = (): Config => ({
    arizona: { login: "", password: "" },
    rodina:  { login: "", password: "" },
    servers: {},
    output_dir: "",
    ai: {
      provider: "gemini",
      openai_api_keys: [""],
      openai_api_key: "",
      openai_model: "gpt-4.1-mini",
      gemini_api_keys: [""],
      gemini_api_key: "",
      gemini_model: "gemini-2.0-flash",
    },
  });

  // ── State ──────────────────────────────────────────────────────────────────
  let cfg: Config = emptyConfig();
  let configPath = "";
  let logLines: { text: string; kind: "info" | "ok" | "err" | "warn" }[] = [];
  let selectedServers = new Set<number>();
  let mode: "uk" | "pdd" | "both" = "both";
  let skipLogin = false;
  let running = false;
  let activeTab: "run" | "config" | "servers" | "ai" = "run";
  let serverSearch = "";
  let expandedServer: string | null = null;
  let showPw = { arizona: false, rodina: false };

  // ── Server constants ───────────────────────────────────────────────────────
  const AZ_PC     = Array.from({ length: 32 }, (_, i) => i + 1);
  const AZ_MOBILE = [101, 102, 103];
  const AZ_VC     = [200];
  const RD_PC     = Array.from({ length: 7 },  (_, i) => i + 301);
  const RD_MOBILE = [401, 402];

  const SERVER_GROUPS = [
    { label: "Arizona PC",     servers: AZ_PC,     project: "arizona" },
    { label: "Arizona Mobile", servers: AZ_MOBILE, project: "arizona" },
    { label: "Arizona VC",     servers: AZ_VC,     project: "arizona" },
    { label: "Rodina PC",      servers: RD_PC,     project: "rodina"  },
    { label: "Rodina Mobile",  servers: RD_MOBILE, project: "rodina"  },
  ];

  // ── Helpers ────────────────────────────────────────────────────────────────
  const log = (text: string, kind: "info" | "ok" | "err" | "warn" = "info") => {
    logLines = [...logLines, { text, kind }];
    // auto-scroll
    setTimeout(() => {
      const el = document.getElementById("log-box");
      if (el) el.scrollTop = el.scrollHeight;
    }, 20);
  };

  const clearLog = () => { logLines = []; };

  const getServerLink = (id: number, field: string): string =>
    (cfg.servers[String(id)] as any)?.[field] ?? "";

  const setServerLink = (id: number, field: string, val: string) => {
    if (!cfg.servers[String(id)]) {
      cfg.servers[String(id)] = { forum_uk_url: "", forum_uk_url_2: "", forum_pdd_url: "", forum_pdd_url_2: "" };
    }
    (cfg.servers[String(id)] as any)[field] = val;
    cfg = cfg; // trigger reactivity
  };

  // Ensure ai.gemini_api_keys / openai_api_keys are arrays (from old configs)
  const normalizeCfg = (c: Config): Config => {
    if (!Array.isArray(c.ai.gemini_api_keys) || c.ai.gemini_api_keys.length === 0) {
      c.ai.gemini_api_keys = c.ai.gemini_api_key ? [c.ai.gemini_api_key] : [""];
    }
    if (!Array.isArray(c.ai.openai_api_keys) || c.ai.openai_api_keys.length === 0) {
      c.ai.openai_api_keys = c.ai.openai_api_key ? [c.ai.openai_api_key] : [""];
    }
    return c;
  };

  const addKey = (provider: "gemini" | "openai") => {
    const arr = provider === "gemini" ? cfg.ai.gemini_api_keys : cfg.ai.openai_api_keys;
    arr.push("");
    cfg = cfg;
  };
  const removeKey = (provider: "gemini" | "openai", idx: number) => {
    const arr = provider === "gemini" ? cfg.ai.gemini_api_keys : cfg.ai.openai_api_keys;
    if (arr.length > 1) { arr.splice(idx, 1); cfg = cfg; }
  };

  // ── Load / Save ─────────────────────────────────────────────────────────────
  const loadAll = async () => {
    try {
      cfg = normalizeCfg((await invoke("load_config")) as Config);
      configPath = (await invoke("get_config_path")) as string;
    } catch (e) {
      log(`Ошибка загрузки конфига: ${e}`, "err");
    }
  };

  const save = async () => {
    try {
      // Sync legacy single-key field so Rust doesn't lose it
      cfg.ai.gemini_api_key  = cfg.ai.gemini_api_keys[0]  ?? "";
      cfg.ai.openai_api_key  = cfg.ai.openai_api_keys[0]  ?? "";
      await invoke("save_config", { config: cfg });
      log("✓ Конфиг сохранён", "ok");
    } catch (e) {
      log(`Ошибка сохранения: ${e}`, "err");
    }
  };

  const pickOutputDir = async () => {
    const result = await openDialog({ directory: true, multiple: false, title: "Выбери папку вывода" });
    if (typeof result === "string") {
      cfg.output_dir = result;
      cfg = cfg;
    }
  };

  // ── Server selection ────────────────────────────────────────────────────────
  const toggleServer = (id: number) => {
    selectedServers.has(id) ? selectedServers.delete(id) : selectedServers.add(id);
    selectedServers = selectedServers;
  };
  const selectGroup = (ids: number[]) => { selectedServers = new Set(ids); };
  const selectAll   = () => { selectedServers = new Set(SERVER_GROUPS.flatMap(g => g.servers)); };
  const clearSel    = () => { selectedServers = new Set(); };

  // ── Run update ──────────────────────────────────────────────────────────────
  const runUpdate = async () => {
    if (selectedServers.size === 0) { log("Не выбраны серверы!", "warn"); return; }
    running = true;
    activeTab = "run";
    log(`▶ Запуск: ${selectedServers.size} серв., режим=${mode.toUpperCase()}`);

    for (const id of Array.from(selectedServers).sort((a, b) => a - b)) {
      try {
        await invoke("run_update", { serverNum: id, mode, skipLogin });
        log(`✓ Сервер ${id} готов`, "ok");
      } catch (e) {
        log(`✗ Сервер ${id}: ${e}`, "err");
      }
    }

    log("■ Обновление завершено", "ok");
    running = false;
  };

  // ── Filtered server list for Servers tab ────────────────────────────────────
  $: filteredGroups = serverSearch.trim()
    ? SERVER_GROUPS.map(g => ({
        ...g,
        servers: g.servers.filter(id => String(id).includes(serverSearch.trim()))
      })).filter(g => g.servers.length > 0)
    : SERVER_GROUPS;

  // ── Tab / mode helpers (no TypeScript casts in Svelte templates) ───────────
  const setTab = (id: string) => { activeTab = id as "run" | "config" | "servers" | "ai"; };
  const setMode = (val: string) => { mode = val as "uk" | "pdd" | "both"; };

  // ── Mount ────────────────────────────────────────────────────────────────────
  onMount(async () => {
    await loadAll();

    const unsub1 = await listen<string>("log", e => {
      const txt = e.payload;
      const kind = txt.startsWith("✓") || txt.startsWith("Saved") ? "ok"
                 : txt.startsWith("✗") || txt.toLowerCase().includes("error") || txt.toLowerCase().includes("failed") ? "err"
                 : txt.startsWith("⚠") ? "warn"
                 : "info";
      log(txt, kind);
    });

    return () => { unsub1(); };
  });
</script>

<!-- ═══════════════════════════════════════════════════════════════════════ -->

<div class="app">

  <!-- ── Sidebar ─────────────────────────────────────────────────────────── -->
  <aside class="sidebar">
    <div class="logo">
      <span class="logo-icon">⚙</span>
      <span class="logo-text">Smart Config</span>
    </div>

    <nav>
      {#each [
        { id: "run",     icon: "▶", label: "Запуск"    },
        { id: "config",  icon: "🔑", label: "Настройки" },
        { id: "servers", icon: "🖥", label: "Серверы"   },
        { id: "ai",      icon: "🤖", label: "AI"        },
      ] as tab}
        <button
          class="nav-btn"
          class:active={activeTab === tab.id}
          on:click={() => setTab(tab.id)}
        >
          <span class="nav-icon">{tab.icon}</span>
          <span>{tab.label}</span>
        </button>
      {/each}
    </nav>

    <div class="sidebar-footer">
      <div class="cfg-path" title={configPath}>{configPath.split("/").pop() || "config.json"}</div>
    </div>
  </aside>

  <!-- ── Main ────────────────────────────────────────────────────────────── -->
  <main>

    <!-- ════════════ RUN TAB ════════════ -->
    {#if activeTab === "run"}
      <div class="page-title">Запуск обновления</div>

      <div class="run-top">

        <!-- Mode -->
        <div class="card">
          <div class="card-label">Режим</div>
          <div class="mode-row">
            {#each [["uk","УК"],["pdd","ПДД"],["both","Оба"]] as [val, lbl]}
              <button
                class="mode-btn"
                class:selected={mode === val}
                on:click={() => setMode(val)}
              >{lbl}</button>
            {/each}
          </div>
        </div>

        <!-- Options -->
        <div class="card">
          <div class="card-label">Опции</div>
          <label class="toggle-row">
            <input type="checkbox" bind:checked={skipLogin} />
            <span>Пропустить логин на форуме</span>
          </label>
        </div>

        <!-- Server selection -->
        <div class="card card-wide">
          <div class="card-label">Серверы</div>
          <div class="sel-actions">
            {#each SERVER_GROUPS as g}
              <button class="sel-btn" on:click={() => selectGroup(g.servers)}>{g.label}</button>
            {/each}
            <button class="sel-btn" on:click={selectAll}>Все</button>
            <button class="sel-btn red" on:click={clearSel}>Очистить</button>
          </div>

          <div class="server-chips">
            {#each SERVER_GROUPS as g}
              {#each g.servers as id}
                <button
                  class="chip"
                  class:on={selectedServers.has(id)}
                  on:click={() => toggleServer(id)}
                >{id}</button>
              {/each}
            {/each}
          </div>

          <div class="sel-stat">
            Выбрано: <strong>{selectedServers.size}</strong> серв.
            {#if selectedServers.size > 0}
              — {Array.from(selectedServers).sort((a,b)=>a-b).slice(0,10).join(", ")}{selectedServers.size > 10 ? "..." : ""}
            {/if}
          </div>
        </div>

      </div>

      <!-- Run button -->
      <button class="run-btn" on:click={runUpdate} disabled={running}>
        {#if running}
          <span class="spinner"></span> Обновление...
        {:else}
          ▶ Запустить
        {/if}
      </button>

      <!-- Log -->
      <div class="log-panel">
        <div class="log-header">
          <span>Лог</span>
          <button class="log-clear" on:click={clearLog}>Очистить</button>
        </div>
        <div id="log-box" class="log-box">
          {#each logLines as line}
            <div class="log-line {line.kind}">{line.text}</div>
          {/each}
          {#if logLines.length === 0}
            <div class="log-line muted">Лог пуст. Запусти обновление.</div>
          {/if}
        </div>
      </div>
    {/if}

    <!-- ════════════ CONFIG TAB ════════════ -->
    {#if activeTab === "config"}
      <div class="page-title">Настройки</div>

      <!-- Output dir -->
      <div class="card">
        <div class="card-label">Папка вывода</div>
        <div class="row-input">
          <input class="inp" value={cfg.output_dir} on:input={e => cfg.output_dir = e.currentTarget.value} placeholder="~/.smart-config-editor" />
          <button class="btn-icon" on:click={pickOutputDir}>📂</button>
          <button class="btn-icon" on:click={() => openShell(cfg.output_dir || configPath.replace(/[^/]+$/, ""))}>↗</button>
        </div>
      </div>

      <!-- Arizona -->
      <div class="card">
        <div class="card-label">Arizona — аккаунт форума</div>
        <div class="creds-grid">
          <div>
            <div class="field-label">Логин</div>
            <input class="inp" bind:value={cfg.arizona.login} placeholder="username" autocomplete="off" />
          </div>
          <div>
            <div class="field-label">Пароль</div>
            <div class="row-input">
              <input class="inp" type={showPw.arizona ? "text" : "password"} bind:value={cfg.arizona.password} autocomplete="new-password" />
              <button class="btn-icon" on:click={() => showPw.arizona = !showPw.arizona}>{showPw.arizona ? "🙈" : "👁"}</button>
            </div>
          </div>
        </div>
      </div>

      <!-- Rodina -->
      <div class="card">
        <div class="card-label">Rodina — аккаунт форума</div>
        <div class="creds-grid">
          <div>
            <div class="field-label">Логин</div>
            <input class="inp" bind:value={cfg.rodina.login} placeholder="username" autocomplete="off" />
          </div>
          <div>
            <div class="field-label">Пароль</div>
            <div class="row-input">
              <input class="inp" type={showPw.rodina ? "text" : "password"} bind:value={cfg.rodina.password} autocomplete="new-password" />
              <button class="btn-icon" on:click={() => showPw.rodina = !showPw.rodina}>{showPw.rodina ? "🙈" : "👁"}</button>
            </div>
          </div>
        </div>
      </div>

      <div class="save-row">
        <button class="btn-primary" on:click={save}>💾 Сохранить</button>
        <button class="btn-secondary" on:click={loadAll}>↺ Перезагрузить</button>
        <button class="btn-ghost" on:click={() => openShell(configPath)}>📝 Открыть config.json</button>
      </div>
    {/if}

    <!-- ════════════ SERVERS TAB ════════════ -->
    {#if activeTab === "servers"}
      <div class="page-title">Ссылки на форум по серверам</div>

      <div class="search-row">
        <input class="inp search-inp" bind:value={serverSearch} placeholder="Поиск по номеру сервера..." />
        {#if serverSearch}
          <button class="btn-icon" on:click={() => serverSearch = ""}>✕</button>
        {/if}
      </div>

      {#each filteredGroups as g}
        <div class="server-group">
          <div class="group-header">{g.label}</div>
          {#each g.servers as id}
            {@const srv = cfg.servers[String(id)] ?? { forum_uk_url:"",forum_uk_url_2:"",forum_pdd_url:"",forum_pdd_url_2:"" }}
            <div class="server-row">
              <button
                class="srv-id"
                class:has-links={srv.forum_uk_url || srv.forum_pdd_url}
                on:click={() => expandedServer = expandedServer === String(id) ? null : String(id)}
              >
                <span class="srv-num">{id}</span>
                <span class="srv-status">
                  {#if srv.forum_uk_url && srv.forum_pdd_url}✅{:else if srv.forum_uk_url || srv.forum_pdd_url}⚡{:else}—{/if}
                </span>
                <span class="srv-arrow">{expandedServer === String(id) ? "▲" : "▼"}</span>
              </button>

              {#if expandedServer === String(id)}
                <div class="srv-fields">
                  {#each [
                    ["forum_uk_url",  "УК — основная ссылка"],
                    ["forum_uk_url_2","УК — доп. ссылка (опц.)"],
                    ["forum_pdd_url", "ПДД/АК — основная ссылка"],
                    ["forum_pdd_url_2","ПДД/АК — доп. ссылка (опц.)"],
                  ] as [field, label]}
                    <div class="srv-field-row">
                      <div class="field-label">{label}</div>
                      <div class="row-input">
                        <input
                          class="inp"
                          value={getServerLink(id, field)}
                          on:input={e => setServerLink(id, field, e.currentTarget.value)}
                          placeholder="https://forum.arizona-rp.com/threads/..."
                        />
                        {#if getServerLink(id, field)}
                          <button class="btn-icon" on:click={() => openShell(getServerLink(id, field))}>↗</button>
                        {/if}
                      </div>
                    </div>
                  {/each}
                </div>
              {/if}
            </div>
          {/each}
        </div>
      {/each}

      <div class="save-row">
        <button class="btn-primary" on:click={save}>💾 Сохранить</button>
      </div>
    {/if}

    <!-- ════════════ AI TAB ════════════ -->
    {#if activeTab === "ai"}
      <div class="page-title">AI-настройки</div>

      <!-- Provider -->
      <div class="card">
        <div class="card-label">Провайдер</div>
        <div class="mode-row">
          <button class="mode-btn" class:selected={cfg.ai.provider === "gemini"} on:click={() => cfg.ai.provider = "gemini"}>Gemini</button>
          <button class="mode-btn" class:selected={cfg.ai.provider === "openai"} on:click={() => cfg.ai.provider = "openai"}>OpenAI</button>
        </div>
      </div>

      <!-- Gemini keys -->
      <div class="card" class:dim={cfg.ai.provider !== "gemini"}>
        <div class="card-label">Gemini API ключи</div>
        <div class="field-label">Модель</div>
        <input class="inp" bind:value={cfg.ai.gemini_model} placeholder="gemini-2.0-flash" style="margin-bottom:12px" />

        {#each cfg.ai.gemini_api_keys as _, i}
          <div class="key-row">
            <div class="field-label">Ключ #{i + 1}</div>
            <div class="row-input">
              <input class="inp" type="password" bind:value={cfg.ai.gemini_api_keys[i]} placeholder="AIza..." autocomplete="new-password" />
              <button class="btn-icon red" on:click={() => removeKey("gemini", i)} disabled={cfg.ai.gemini_api_keys.length <= 1}>✕</button>
            </div>
          </div>
        {/each}
        <button class="btn-ghost small" on:click={() => addKey("gemini")}>+ Добавить ключ</button>
      </div>

      <!-- OpenAI keys -->
      <div class="card" class:dim={cfg.ai.provider !== "openai"}>
        <div class="card-label">OpenAI API ключи</div>
        <div class="field-label">Модель</div>
        <input class="inp" bind:value={cfg.ai.openai_model} placeholder="gpt-4.1-mini" style="margin-bottom:12px" />

        {#each cfg.ai.openai_api_keys as _, i}
          <div class="key-row">
            <div class="field-label">Ключ #{i + 1}</div>
            <div class="row-input">
              <input class="inp" type="password" bind:value={cfg.ai.openai_api_keys[i]} placeholder="sk-..." autocomplete="new-password" />
              <button class="btn-icon red" on:click={() => removeKey("openai", i)} disabled={cfg.ai.openai_api_keys.length <= 1}>✕</button>
            </div>
          </div>
        {/each}
        <button class="btn-ghost small" on:click={() => addKey("openai")}>+ Добавить ключ</button>
      </div>

      <div class="save-row">
        <button class="btn-primary" on:click={save}>💾 Сохранить</button>
      </div>
    {/if}

  </main>
</div>

<!-- ═══════════════════════════════════════════════════════════════════════ -->
<style>
  :global(*) { box-sizing: border-box; margin: 0; padding: 0; }
  :global(body) {
    font-family: "Inter", "Segoe UI", system-ui, sans-serif;
    background: #0d1117;
    color: #e6edf3;
    height: 100vh;
    overflow: hidden;
  }

  /* ── Layout ─────────────────────────────────────────────────────────── */
  .app {
    display: flex;
    height: 100vh;
  }

  /* ── Sidebar ─────────────────────────────────────────────────────────── */
  .sidebar {
    width: 180px;
    flex-shrink: 0;
    background: #161b22;
    border-right: 1px solid #30363d;
    display: flex;
    flex-direction: column;
    padding: 16px 0;
    gap: 4px;
  }

  .logo {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 0 16px 16px;
    border-bottom: 1px solid #30363d;
    margin-bottom: 8px;
  }
  .logo-icon { font-size: 20px; }
  .logo-text { font-size: 13px; font-weight: 700; color: #58a6ff; letter-spacing: 0.3px; }

  nav { flex: 1; display: flex; flex-direction: column; gap: 2px; padding: 0 8px; }

  .nav-btn {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 9px 10px;
    border: none;
    border-radius: 6px;
    background: transparent;
    color: #8b949e;
    cursor: pointer;
    font-size: 13px;
    font-weight: 500;
    transition: all 0.15s;
    text-align: left;
  }
  .nav-btn:hover { background: #21262d; color: #e6edf3; }
  .nav-btn.active { background: #1f6feb; color: #fff; }
  .nav-icon { font-size: 15px; width: 20px; text-align: center; }

  .sidebar-footer {
    padding: 12px 12px 0;
    border-top: 1px solid #30363d;
  }
  .cfg-path {
    font-size: 10px;
    color: #484f58;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  /* ── Main ─────────────────────────────────────────────────────────────── */
  main {
    flex: 1;
    overflow-y: auto;
    padding: 20px 24px;
    display: flex;
    flex-direction: column;
    gap: 14px;
  }

  .page-title {
    font-size: 18px;
    font-weight: 700;
    color: #e6edf3;
    padding-bottom: 4px;
    border-bottom: 1px solid #30363d;
    margin-bottom: 2px;
  }

  /* ── Card ─────────────────────────────────────────────────────────────── */
  .card {
    background: #161b22;
    border: 1px solid #30363d;
    border-radius: 10px;
    padding: 14px 16px;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .card.dim { opacity: 0.5; pointer-events: none; }
  .card-wide { /* same */ }
  .card-label {
    font-size: 11px;
    font-weight: 700;
    color: #58a6ff;
    text-transform: uppercase;
    letter-spacing: 0.6px;
  }
  .field-label {
    font-size: 11px;
    color: #8b949e;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.4px;
    margin-bottom: 4px;
  }

  /* ── Run tab layout ──────────────────────────────────────────────────── */
  .run-top {
    display: grid;
    grid-template-columns: auto auto 1fr;
    gap: 14px;
  }

  /* ── Mode buttons ─────────────────────────────────────────────────────── */
  .mode-row { display: flex; gap: 6px; }
  .mode-btn {
    padding: 7px 18px;
    border: 1px solid #30363d;
    border-radius: 6px;
    background: #21262d;
    color: #8b949e;
    font-weight: 600;
    font-size: 13px;
    cursor: pointer;
    transition: all 0.15s;
  }
  .mode-btn:hover { border-color: #58a6ff; color: #e6edf3; }
  .mode-btn.selected { background: #1f6feb; border-color: #1f6feb; color: #fff; }

  /* ── Toggle ───────────────────────────────────────────────────────────── */
  .toggle-row {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 13px;
    color: #c9d1d9;
    cursor: pointer;
    user-select: none;
  }
  .toggle-row input { accent-color: #58a6ff; width: 15px; height: 15px; cursor: pointer; }

  /* ── Server selection ─────────────────────────────────────────────────── */
  .sel-actions { display: flex; flex-wrap: wrap; gap: 6px; }
  .sel-btn {
    padding: 5px 12px;
    border: 1px solid #30363d;
    border-radius: 5px;
    background: #21262d;
    color: #8b949e;
    font-size: 12px;
    font-weight: 600;
    cursor: pointer;
    transition: all 0.15s;
  }
  .sel-btn:hover { border-color: #58a6ff; color: #e6edf3; }
  .sel-btn.red { border-color: #f85149; color: #f85149; }
  .sel-btn.red:hover { background: rgba(248,81,73,0.15); }

  .server-chips { display: flex; flex-wrap: wrap; gap: 5px; max-height: 120px; overflow-y: auto; }
  .chip {
    padding: 3px 9px;
    border: 1px solid #30363d;
    border-radius: 4px;
    background: #21262d;
    color: #8b949e;
    font-size: 12px;
    cursor: pointer;
    transition: all 0.12s;
    font-weight: 500;
  }
  .chip:hover { border-color: #58a6ff; }
  .chip.on { background: #1f6feb; border-color: #1f6feb; color: #fff; }

  .sel-stat { font-size: 12px; color: #8b949e; }
  .sel-stat strong { color: #58a6ff; }

  /* ── Run button ────────────────────────────────────────────────────────── */
  .run-btn {
    align-self: flex-start;
    padding: 11px 32px;
    border: none;
    border-radius: 8px;
    background: #238636;
    color: #fff;
    font-size: 15px;
    font-weight: 700;
    cursor: pointer;
    transition: all 0.2s;
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .run-btn:hover:not(:disabled) { background: #2ea043; transform: translateY(-1px); }
  .run-btn:disabled { opacity: 0.6; cursor: not-allowed; }

  @keyframes spin { to { transform: rotate(360deg); } }
  .spinner {
    width: 14px; height: 14px;
    border: 2px solid rgba(255,255,255,0.3);
    border-top-color: #fff;
    border-radius: 50%;
    animation: spin 0.8s linear infinite;
  }

  /* ── Log ─────────────────────────────────────────────────────────────── */
  .log-panel {
    flex: 1;
    display: flex;
    flex-direction: column;
    min-height: 200px;
    background: #161b22;
    border: 1px solid #30363d;
    border-radius: 10px;
    overflow: hidden;
  }
  .log-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 8px 12px;
    background: #1c2128;
    border-bottom: 1px solid #30363d;
    font-size: 12px;
    font-weight: 700;
    color: #8b949e;
    text-transform: uppercase;
    letter-spacing: 0.5px;
  }
  .log-clear {
    padding: 2px 10px;
    border: 1px solid #30363d;
    border-radius: 4px;
    background: transparent;
    color: #8b949e;
    font-size: 11px;
    cursor: pointer;
  }
  .log-clear:hover { color: #e6edf3; border-color: #8b949e; }
  .log-box {
    flex: 1;
    overflow-y: auto;
    padding: 10px 12px;
    font-family: "Cascadia Code", "Fira Mono", "IBM Plex Mono", monospace;
    font-size: 12px;
    line-height: 1.6;
  }
  .log-line { color: #c9d1d9; }
  .log-line.ok   { color: #3fb950; }
  .log-line.err  { color: #f85149; }
  .log-line.warn { color: #d29922; }
  .log-line.muted { color: #484f58; font-style: italic; }

  /* ── Inputs ─────────────────────────────────────────────────────────── */
  .inp {
    width: 100%;
    background: #0d1117;
    border: 1px solid #30363d;
    border-radius: 6px;
    color: #e6edf3;
    padding: 7px 10px;
    font-size: 13px;
    outline: none;
    transition: border-color 0.15s;
  }
  .inp:focus { border-color: #58a6ff; }
  .inp::placeholder { color: #484f58; }

  .row-input { display: flex; gap: 6px; align-items: center; }
  .row-input .inp { flex: 1; }

  .creds-grid { display: grid; grid-template-columns: 1fr 1fr; gap: 12px; }

  .btn-icon {
    padding: 7px 10px;
    border: 1px solid #30363d;
    border-radius: 6px;
    background: #21262d;
    color: #8b949e;
    cursor: pointer;
    font-size: 14px;
    transition: all 0.15s;
    white-space: nowrap;
  }
  .btn-icon:hover { border-color: #58a6ff; color: #e6edf3; }
  .btn-icon.red { border-color: #f85149; color: #f85149; }
  .btn-icon.red:hover { background: rgba(248,81,73,0.15); }
  .btn-icon:disabled { opacity: 0.3; cursor: not-allowed; }

  .btn-primary {
    padding: 9px 20px;
    border: none;
    border-radius: 7px;
    background: #238636;
    color: #fff;
    font-size: 13px;
    font-weight: 700;
    cursor: pointer;
    transition: all 0.15s;
  }
  .btn-primary:hover { background: #2ea043; }

  .btn-secondary {
    padding: 9px 20px;
    border: 1px solid #30363d;
    border-radius: 7px;
    background: transparent;
    color: #8b949e;
    font-size: 13px;
    font-weight: 600;
    cursor: pointer;
    transition: all 0.15s;
  }
  .btn-secondary:hover { color: #e6edf3; border-color: #8b949e; }

  .btn-ghost {
    padding: 9px 20px;
    border: 1px solid #30363d;
    border-radius: 7px;
    background: transparent;
    color: #8b949e;
    font-size: 13px;
    cursor: pointer;
    transition: all 0.15s;
  }
  .btn-ghost:hover { border-color: #58a6ff; color: #58a6ff; }
  .btn-ghost.small { padding: 5px 14px; font-size: 12px; }

  .save-row { display: flex; gap: 10px; margin-top: 4px; }

  /* ── Servers tab ─────────────────────────────────────────────────────── */
  .search-row { display: flex; gap: 8px; align-items: center; }
  .search-inp { max-width: 280px; }

  .server-group { margin-bottom: 16px; }
  .group-header {
    font-size: 11px;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.6px;
    color: #8b949e;
    margin-bottom: 6px;
    padding: 4px 0;
    border-bottom: 1px solid #21262d;
  }

  .server-row { margin-bottom: 4px; }
  .srv-id {
    display: flex;
    align-items: center;
    gap: 10px;
    width: 100%;
    padding: 8px 12px;
    border: 1px solid #30363d;
    border-radius: 6px;
    background: #161b22;
    color: #c9d1d9;
    cursor: pointer;
    text-align: left;
    font-size: 13px;
    font-weight: 500;
    transition: all 0.15s;
  }
  .srv-id:hover { border-color: #58a6ff; }
  .srv-id.has-links { border-color: #238636; }
  .srv-num { font-weight: 700; min-width: 36px; }
  .srv-status { margin-left: 4px; }
  .srv-arrow { margin-left: auto; color: #8b949e; font-size: 11px; }

  .srv-fields {
    background: #0d1117;
    border: 1px solid #30363d;
    border-top: none;
    border-radius: 0 0 6px 6px;
    padding: 12px 12px;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .srv-field-row { display: flex; flex-direction: column; gap: 4px; }

  /* ── AI tab ──────────────────────────────────────────────────────────── */
  .key-row { margin-bottom: 6px; }
</style>
