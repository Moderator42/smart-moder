<script lang="ts">
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { open as openShell } from "@tauri-apps/plugin-shell";
  import { open as openDialog } from "@tauri-apps/plugin-dialog";

  type ServerLinks = {
    forum_uk_urls: string[];
    forum_pdd_urls: string[];
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
      provider: "gemini" | "openai" | string;
      backend_url: string;
      backend_token: string;
      openai_api_keys: string[];
      openai_api_key: string;
      openai_model: string;
      gemini_api_keys: string[];
      gemini_api_key: string;
      gemini_model: string;
    };
  };

  type HistoryEntry = {
    id: string | number;
    created_at: number;
    user: string;
    server: string;
    server_id?: number;
    mode: string;
    ai_provider: string;
    status: string;
    limit_charged: boolean;
    file_saved: boolean;
    message: string;
  };

  type DiffReport = {
    created_at: number;
    server: string;
    mode: string;
    added: string[];
    changed: string[];
    removed: string[];
  };

  type BackendUser = {
    id: number;
    telegram_id: number;
    username?: string | null;
    role: "admin" | "editor" | "user";
    daily_limit: number;
    used_today?: number;
    left_today?: number;
  };

  type BackendLink = {
    id: number;
    server_id: number;
    type: "UK" | "PDD";
    title: string;
    url: string;
    priority: number;
    is_active: boolean;
  };

  type BackendServer = {
    id: number;
    name: string;
    is_active: boolean;
    links: BackendLink[];
  };

  const emptyLinks = (): ServerLinks => ({
    forum_uk_urls: [],
    forum_pdd_urls: [],
    forum_uk_url: "",
    forum_uk_url_2: "",
    forum_pdd_url: "",
    forum_pdd_url_2: "",
  });

  const emptyConfig = (): Config => ({
    arizona: { login: "", password: "" },
    rodina: { login: "", password: "" },
    servers: {},
    output_dir: "",
    ai: {
      provider: "gemini",
      backend_url: "https://api.smart.moder42.tech",
      backend_token: "",
      openai_api_keys: [""],
      openai_api_key: "",
      openai_model: "gpt-4.1-mini",
      gemini_api_keys: [""],
      gemini_api_key: "",
      gemini_model: "gemini-2.0-flash",
    },
  });

  let cfg: Config = emptyConfig();
  let configPath = "";
  let logLines: { text: string; kind: "info" | "ok" | "err" | "warn" }[] = [];
  let selectedServers = new Set<number>();
  let mode: "uk" | "pdd" | "both" = "both";
  let skipLogin = false;
  let running = false;
  let activeTab: "dashboard" | "history" | "diff" | "backups" | "servers" | "config" | "ai" | "profile" = "dashboard";
  let serverSearch = "";
  let history: HistoryEntry[] = [];
  let diff: DiffReport | null = null;
  let backendUser: BackendUser | null = null;
  let backendServers: BackendServer[] = [];
  let loginCode = "";
  let backendError = "";
  let showPw = { arizona: false, rodina: false };

  const AZ_PC = Array.from({ length: 32 }, (_, i) => i + 1);
  const AZ_MOBILE = [101, 102, 103];
  const AZ_VC = [200];
  const RD_PC = Array.from({ length: 7 }, (_, i) => i + 301);
  const RD_MOBILE = [401, 402];

  const LOCAL_GROUPS = [
    { label: "Arizona PC", servers: AZ_PC, project: "arizona" },
    { label: "Arizona Mobile", servers: AZ_MOBILE, project: "arizona" },
    { label: "Arizona VC", servers: AZ_VC, project: "arizona" },
    { label: "Rodina PC", servers: RD_PC, project: "rodina" },
    { label: "Rodina Mobile", servers: RD_MOBILE, project: "rodina" },
  ];

  const tabs = [
    { id: "dashboard", icon: "🏠", label: "Главная" },
    { id: "history", icon: "📜", label: "История" },
    { id: "diff", icon: "🧩", label: "Diff" },
    { id: "backups", icon: "🗂", label: "Backups" },
    { id: "servers", icon: "🖥", label: "Серверы" },
    { id: "config", icon: "⚙", label: "Настройки" },
    { id: "ai", icon: "🤖", label: "AI" },
    { id: "profile", icon: "👤", label: "Профиль" },
  ];

  const log = (text: string, kind: "info" | "ok" | "err" | "warn" = "info") => {
    logLines = [...logLines, { text, kind }];
    setTimeout(() => {
      const el = document.getElementById("log-box");
      if (el) el.scrollTop = el.scrollHeight;
    }, 20);
  };

  const normalizeLinks = (links?: Partial<ServerLinks>): ServerLinks => {
    const out = { ...emptyLinks(), ...(links ?? {}) } as ServerLinks;
    if (!Array.isArray(out.forum_uk_urls) || out.forum_uk_urls.length === 0) {
      out.forum_uk_urls = [out.forum_uk_url, out.forum_uk_url_2].filter(Boolean);
    }
    if (!Array.isArray(out.forum_pdd_urls) || out.forum_pdd_urls.length === 0) {
      out.forum_pdd_urls = [out.forum_pdd_url, out.forum_pdd_url_2].filter(Boolean);
    }
    out.forum_uk_url = out.forum_uk_urls[0] ?? "";
    out.forum_uk_url_2 = out.forum_uk_urls[1] ?? "";
    out.forum_pdd_url = out.forum_pdd_urls[0] ?? "";
    out.forum_pdd_url_2 = out.forum_pdd_urls[1] ?? "";
    return out;
  };

  const normalizeCfg = (c: Config): Config => {
    if (!c.ai) c.ai = emptyConfig().ai;
    if (!c.ai.backend_url) c.ai.backend_url = "https://api.smart.moder42.tech";
    if (!Array.isArray(c.ai.gemini_api_keys) || c.ai.gemini_api_keys.length === 0) c.ai.gemini_api_keys = c.ai.gemini_api_key ? [c.ai.gemini_api_key] : [""];
    if (!Array.isArray(c.ai.openai_api_keys) || c.ai.openai_api_keys.length === 0) c.ai.openai_api_keys = c.ai.openai_api_key ? [c.ai.openai_api_key] : [""];
    for (const key of Object.keys(c.servers ?? {})) c.servers[key] = normalizeLinks(c.servers[key]);
    return c;
  };

  const api = async (path: string, options: RequestInit = {}) => {
    const headers: Record<string, string> = { "Content-Type": "application/json" };
    if (cfg.ai.backend_token) headers.Authorization = `Bearer ${cfg.ai.backend_token}`;
    const res = await fetch(`${cfg.ai.backend_url.replace(/\/$/, "")}${path}`, {
      ...options,
      headers: { ...headers, ...(options.headers as Record<string, string> | undefined) },
    });
    const text = await res.text();
    let data: any = null;
    try { data = text ? JSON.parse(text) : null; } catch { data = null; }
    if (!res.ok) throw new Error(data?.detail ?? text ?? `HTTP ${res.status}`);
    return data;
  };

  const syncBackendServersToLocalConfig = () => {
    if (backendServers.length === 0) return;
    for (const server of backendServers) {
      const uk = server.links.filter(l => l.type === "UK" && l.is_active).sort((a, b) => a.priority - b.priority).map(l => l.url);
      const pdd = server.links.filter(l => l.type === "PDD" && l.is_active).sort((a, b) => a.priority - b.priority).map(l => l.url);
      cfg.servers[String(server.id)] = normalizeLinks({ forum_uk_urls: uk, forum_pdd_urls: pdd });
    }
    cfg = cfg;
  };

  const save = async () => {
    cfg.ai.gemini_api_key = cfg.ai.gemini_api_keys[0] ?? "";
    // OpenAI ключи специально не сохраняем локально: OpenAI работает через backend.
    cfg.ai.openai_api_key = "";
    cfg.ai.openai_api_keys = [""];
    for (const key of Object.keys(cfg.servers)) cfg.servers[key] = normalizeLinks(cfg.servers[key]);
    await invoke("save_config", { config: cfg });
    log("✓ Конфиг сохранён", "ok");
  };

  const loadBackend = async () => {
    backendError = "";
    if (!cfg.ai.backend_token) return;
    try {
      backendUser = await api("/auth/me");
      backendServers = await api("/servers");
      syncBackendServersToLocalConfig();
      await invoke("save_config", { config: cfg });
    } catch (e) {
      backendUser = null;
      backendServers = [];
      backendError = String(e);
    }
  };

  const refreshHistory = async () => {
    if (cfg.ai.backend_token) {
      try {
        history = await api("/history");
        return;
      } catch (e) {
        log(`История backend недоступна: ${e}`, "warn");
      }
    }
    history = (await invoke("get_history")) as HistoryEntry[];
  };

  const refreshDiff = async () => {
    diff = ((await invoke("get_last_diff")) as DiffReport | null) ?? null;
  };

  const loadAll = async () => {
    try {
      cfg = normalizeCfg((await invoke("load_config")) as Config);
      configPath = (await invoke("get_config_path")) as string;
      await loadBackend();
      await refreshHistory();
      await refreshDiff();
    } catch (e) {
      log(`Ошибка загрузки: ${e}`, "err");
    }
  };

  const loginWithTelegramCode = async () => {
    try {
      const data = await api("/auth/telegram-code", { method: "POST", body: JSON.stringify({ code: loginCode.trim().toUpperCase() }) });
      cfg.ai.backend_token = data.token;
      backendUser = data.user;
      loginCode = "";
      await save();
      await loadBackend();
      await refreshHistory();
      log("✓ Вход через Telegram выполнен", "ok");
    } catch (e) {
      log(`Ошибка входа: ${e}`, "err");
    }
  };

  const logout = async () => {
    cfg.ai.backend_token = "";
    backendUser = null;
    backendServers = [];
    await save();
    await refreshHistory();
  };

  const pickOutputDir = async () => {
    const result = await openDialog({ directory: true, multiple: false, title: "Выбери папку вывода" });
    if (typeof result === "string") {
      cfg.output_dir = result;
      cfg = cfg;
    }
  };

  const localServers = LOCAL_GROUPS.flatMap(g => g.servers.map(id => ({ id, name: String(id), group: g.label })));
  $: runtimeServers = backendServers.length > 0
    ? backendServers.map(s => ({ id: s.id, name: s.name, group: "Backend" }))
    : localServers;
  $: filteredServers = runtimeServers.filter(s => !serverSearch.trim() || s.name.toLowerCase().includes(serverSearch.toLowerCase()) || String(s.id).includes(serverSearch.trim()));
  $: selectedPreview = Array.from(selectedServers).sort((a, b) => a - b).slice(0, 12).join(", ");
  $: dailyLimitText = cfg.ai.provider === "gemini" ? "∞" : `${backendUser?.left_today ?? "?"}/${backendUser?.daily_limit ?? 4}`;

  const toggleServer = (id: number) => {
    selectedServers.has(id) ? selectedServers.delete(id) : selectedServers.add(id);
    selectedServers = selectedServers;
  };
  const selectAll = () => { selectedServers = new Set(filteredServers.map(s => s.id)); };
  const clearSel = () => { selectedServers = new Set(); };
  const setMode = (val: string) => { mode = val as any; };
  const setTab = (id: string) => { activeTab = id as any; };

  const postIncident = async (serverId: number, message: string) => {
    if (!cfg.ai.backend_token) return;
    try {
      await api("/incidents", { method: "POST", body: JSON.stringify({ server_id: serverId, message, type: "parse_error" }) });
    } catch (e) {
      log(`Не удалось отправить incident: ${e}`, "warn");
    }
  };

  const runUpdate = async () => {
    if (selectedServers.size === 0) { log("Выбери хотя бы один сервер", "warn"); return; }
    if (selectedServers.size > 1) { log("Для безопасного diff-before-save выбери один сервер за запуск. Так не появятся скрытые pending-файлы.", "warn"); return; }
    if (cfg.ai.provider === "openai" && !cfg.ai.backend_token) { log("Для OpenAI нужно войти через Telegram/backend", "err"); return; }
    running = true;
    activeTab = "dashboard";
    syncBackendServersToLocalConfig();
    await save();
    log(`▶ Запуск: ${selectedServers.size} сервер(ов), режим ${mode.toUpperCase()}`);

    for (const id of Array.from(selectedServers).sort((a, b) => a - b)) {
      try {
        if (cfg.ai.backend_token) {
          const can = await api("/runs/can-start", { method: "POST", body: JSON.stringify({ server_id: id, mode: mode.toUpperCase(), ai_provider: cfg.ai.provider }) });
          if (!can.allowed) throw new Error(`Лимит на сегодня исчерпан: ${can.used}/${can.daily_limit}`);
        }
        await invoke("run_update", { serverNum: id, mode, skipLogin });
        log(`✓ Сервер ${id}: pending JSON создан, проверь Diff`, "ok");
      } catch (e) {
        log(`✗ Сервер ${id}: ${e}`, "err");
        await postIncident(id, String(e));
        if (cfg.ai.backend_token) {
          try { await api("/runs/finish", { method: "POST", body: JSON.stringify({ server_id: id, mode: mode.toUpperCase(), ai_provider: cfg.ai.provider, status: "failed", file_saved: false, message: String(e) }) }); } catch {}
        }
      }
      await refreshHistory();
      await refreshDiff();
    }

    await loadBackend();
    log("■ Запуск завершён. Сохрани или отмени результат на вкладке Diff.", "ok");
    running = false;
  };

  const applyCurrentDiff = async () => {
    if (!diff) return;
    try {
      const serverNum = Number(diff.server);
      const runMode = diff.mode.toLowerCase();
      await invoke("apply_pending", { serverNum, mode: runMode });
      if (cfg.ai.backend_token) {
        await api("/runs/finish", { method: "POST", body: JSON.stringify({ server_id: serverNum, mode: diff.mode, ai_provider: cfg.ai.provider, status: "success", file_saved: true, message: "JSON сохранён после подтверждения diff" }) });
      }
      log("✓ Pending JSON сохранён", "ok");
      await loadBackend();
      await refreshHistory();
      await refreshDiff();
    } catch (e) {
      log(`Ошибка сохранения pending: ${e}`, "err");
    }
  };

  const cancelCurrentDiff = async () => {
    if (!diff) return;
    try {
      const serverNum = Number(diff.server);
      const runMode = diff.mode.toLowerCase();
      await invoke("cancel_pending", { serverNum, mode: runMode });
      if (cfg.ai.backend_token) {
        await api("/runs/finish", { method: "POST", body: JSON.stringify({ server_id: serverNum, mode: diff.mode, ai_provider: cfg.ai.provider, status: "cancelled", file_saved: false, message: "Пользователь отменил сохранение на diff-экране" }) });
      }
      log("Сохранение отменено", "warn");
      await refreshHistory();
      await refreshDiff();
    } catch (e) {
      log(`Ошибка отмены: ${e}`, "err");
    }
  };

  const ensureLocalLinks = (id: number): ServerLinks => {
    const key = String(id);
    cfg.servers[key] = normalizeLinks(cfg.servers[key]);
    return cfg.servers[key];
  };

  const addLocalUrl = (id: number, type: "uk" | "pdd") => {
    const key = String(id);
    cfg.servers[key] = normalizeLinks(cfg.servers[key]);
    const arr = type === "uk" ? cfg.servers[key].forum_uk_urls : cfg.servers[key].forum_pdd_urls;
    arr.push("");
    cfg = cfg;
  };

  const removeLocalUrl = (id: number, type: "uk" | "pdd", idx: number) => {
    const key = String(id);
    cfg.servers[key] = normalizeLinks(cfg.servers[key]);
    const arr = type === "uk" ? cfg.servers[key].forum_uk_urls : cfg.servers[key].forum_pdd_urls;
    arr.splice(idx, 1);
    cfg = cfg;
  };

  const fmtDate = (ts: number) => new Date(ts * 1000).toLocaleString("ru-RU");
  const statusLabel = (s: string) => s === "success" ? "Успешно" : s === "cancelled" ? "Отменено" : s === "pending" ? "Ожидает" : "Ошибка";

  onMount(async () => {
    await loadAll();
    const unsub = await listen<string>("log", e => {
      const txt = e.payload;
      const lower = txt.toLowerCase();
      const kind = txt.startsWith("✓") || txt.startsWith("Saved") ? "ok"
        : txt.startsWith("✗") || lower.includes("error") || lower.includes("failed") ? "err"
        : txt.startsWith("⚠") ? "warn" : "info";
      log(txt, kind);
    });
    return () => { unsub(); };
  });
</script>

<div class="shell">
  <aside class="sidebar">
    <div class="brand">
      <div class="brand-mark">S</div>
      <div>
        <div class="brand-title">Smart Config</div>
        <div class="brand-sub">Team Edition</div>
      </div>
    </div>

    <nav class="nav">
      {#each tabs as tab}
        <button class="nav-btn" class:active={activeTab === tab.id} on:click={() => setTab(tab.id)}>
          <span>{tab.icon}</span><span>{tab.label}</span>
        </button>
      {/each}
    </nav>

    <div class="profile-card">
      <div class="profile-row"><span>Роль</span><b>{backendUser?.role ?? "local"}</b></div>
      <div class="profile-row"><span>AI</span><b>{cfg.ai.provider || "gemini"}</b></div>
      <div class="profile-row"><span>Лимит</span><b>{dailyLimitText}</b></div>
    </div>
  </aside>

  <main class="main">
    {#if activeTab === "dashboard"}
      <section class="hero">
        <div>
          <p class="eyebrow">Панель проверки</p>
          <h1>Обновление SmartUK / SmartPDD</h1>
          <p>OpenAI работает через backend и списывает лимит только после сохранения diff. Gemini хранится локально и лимиты не списывает.</p>
        </div>
        <button class="primary big" on:click={runUpdate} disabled={running}>{running ? "Выполняется..." : "Запустить проверку"}</button>
      </section>

      {#if !backendUser}
        <section class="panel login-card">
          <h2>Вход через Telegram</h2>
          <p class="muted">Открой бота, нажми /start, получи код и введи его здесь. Для Gemini можно работать локально, но история команды и OpenAI требуют вход.</p>
          <div class="input-row"><input class="input" bind:value={loginCode} placeholder="AB12CD" /><button class="primary" on:click={loginWithTelegramCode}>Войти</button></div>
          {#if backendError}<p class="error-text">{backendError}</p>{/if}
        </section>
      {/if}

      <section class="grid-3">
        <div class="glass-card"><span class="metric">{selectedServers.size}</span><small>выбрано серверов</small></div>
        <div class="glass-card"><span class="metric">{mode.toUpperCase()}</span><small>режим проверки</small></div>
        <div class="glass-card"><span class="metric">{dailyLimitText}</span><small>лимит OpenAI</small></div>
      </section>

      <section class="panel">
        <div class="panel-head"><h2>Режим</h2></div>
        <div class="segmented">
          {#each [["uk","УК"],["pdd","ПДД"],["both","УК + ПДД"]] as [val, label]}
            <button class:selected={mode === val} on:click={() => setMode(val)}>{label}</button>
          {/each}
        </div>
        <label class="check"><input type="checkbox" bind:checked={skipLogin} /> Пропустить логин на форуме</label>
      </section>

      <section class="panel">
        <div class="panel-head">
          <h2>Серверы</h2>
          <div class="actions"><button on:click={selectAll}>Все видимые</button><button on:click={clearSel}>Очистить</button></div>
        </div>
        <input class="input search" bind:value={serverSearch} placeholder="Поиск сервера" />
        <div class="cards-grid">
          {#each filteredServers as srv}
            <button class="server-card" class:on={selectedServers.has(srv.id)} on:click={() => toggleServer(srv.id)}>
              <b>{srv.name}</b>
              <span>#{srv.id}</span>
            </button>
          {/each}
        </div>
        <p class="muted">Выбрано: {selectedPreview}{selectedServers.size > 12 ? "..." : ""}</p>
      </section>

      <section class="log-panel">
        <div class="panel-head"><h2>Лог</h2><button on:click={() => logLines = []}>Очистить</button></div>
        <div id="log-box" class="log-box">
          {#each logLines as line}<div class="log-line {line.kind}">{line.text}</div>{/each}
          {#if logLines.length === 0}<div class="log-line muted">Лог пуст.</div>{/if}
        </div>
      </section>
    {/if}

    {#if activeTab === "history"}
      <section class="title-row"><div><p class="eyebrow">Последние 25</p><h1>История запусков</h1></div><button on:click={refreshHistory}>Обновить</button></section>
      <section class="panel table-wrap">
        <table>
          <thead><tr><th>Дата</th><th>Пользователь</th><th>Сервер</th><th>Тип</th><th>AI</th><th>Статус</th><th>Лимит</th><th>Сохранено</th></tr></thead>
          <tbody>
            {#each history as h}
              <tr>
                <td>{fmtDate(h.created_at)}</td><td>{h.user}</td><td>{h.server}</td><td>{h.mode}</td><td>{h.ai_provider}</td>
                <td><span class="badge {h.status}">{statusLabel(h.status)}</span></td>
                <td>{h.limit_charged ? "Да" : "Нет"}</td><td>{h.file_saved ? "Да" : "Нет"}</td>
              </tr>
            {/each}
          </tbody>
        </table>
        {#if history.length === 0}<p class="muted">Истории пока нет.</p>{/if}
      </section>
    {/if}

    {#if activeTab === "diff"}
      <section class="title-row">
        <div><p class="eyebrow">Предпросмотр изменений</p><h1>Diff последнего запуска</h1></div>
        <div class="actions"><button on:click={refreshDiff}>Обновить</button>{#if diff}<button class="primary" on:click={applyCurrentDiff}>Сохранить</button><button class="danger" on:click={cancelCurrentDiff}>Отмена</button>{/if}</div>
      </section>
      {#if diff}
        <section class="panel"><b>Сервер #{diff.server}</b> · {diff.mode} · файл ещё не записан, пока не нажмёшь “Сохранить”.</section>
        <section class="grid-3">
          <div class="glass-card add"><span class="metric">+{diff.added.length}</span><small>добавлено</small></div>
          <div class="glass-card change"><span class="metric">~{diff.changed.length}</span><small>изменено</small></div>
          <div class="glass-card remove"><span class="metric">-{diff.removed.length}</span><small>удалено</small></div>
        </section>
        <section class="diff-grid">
          <div class="diff-card add"><h2>Добавлено</h2>{#each diff.added as item}<p>+ {item}</p>{/each}</div>
          <div class="diff-card change"><h2>Изменено</h2>{#each diff.changed as item}<p>~ {item}</p>{/each}</div>
          <div class="diff-card remove"><h2>Удалено</h2>{#each diff.removed as item}<p>- {item}</p>{/each}</div>
        </section>
      {:else}
        <section class="panel"><p class="muted">Diff ещё не создан.</p></section>
      {/if}
    {/if}

    {#if activeTab === "backups"}
      <section class="title-row"><div><p class="eyebrow">Резервные копии</p><h1>Backups</h1></div></section>
      <section class="panel"><p>Перед каждым подтверждённым сохранением файл копируется в подпапку <b>backups</b> рядом с JSON.</p><button on:click={() => openShell(cfg.output_dir || configPath.replace(/[^/]+$/, ""))}>Открыть папку вывода</button></section>
    {/if}

    {#if activeTab === "servers"}
      <section class="title-row"><div><p class="eyebrow">Серверы и ссылки</p><h1>Серверы</h1></div><button class="primary" on:click={loadBackend}>Обновить с backend</button></section>
      {#if backendServers.length > 0}
        <section class="panel"><p>Список управляется через Telegram-бота. Editor/Admin могут добавлять серверы и любое количество UK/PDD-ссылок командами бота.</p></section>
        {#each backendServers as s}
          <section class="panel">
            <h2>{s.name} <span class="muted">#{s.id}</span></h2>
            <div class="links-grid">
              <div><h3>UK</h3>{#each s.links.filter(l => l.type === "UK") as l}<p><b>{l.title || `Ссылка ${l.id}`}</b><br/><span class="muted">{l.url}</span></p>{/each}</div>
              <div><h3>PDD</h3>{#each s.links.filter(l => l.type === "PDD") as l}<p><b>{l.title || `Ссылка ${l.id}`}</b><br/><span class="muted">{l.url}</span></p>{/each}</div>
            </div>
          </section>
        {/each}
      {:else}
        <section class="panel"><p class="muted">Backend не подключён. Ниже локальные ссылки для старого режима.</p></section>
        {#each filteredServers as srv}
          {@const links = ensureLocalLinks(srv.id)}
          <section class="panel">
            <h2>#{srv.id}</h2>
            <div class="links-grid">
              <div>
                <h3>UK</h3>
                {#each links.forum_uk_urls as _, i}
                  <div class="input-row"><input class="input" bind:value={cfg.servers[String(srv.id)].forum_uk_urls[i]} placeholder="https://forum..." /><button on:click={() => removeLocalUrl(srv.id, "uk", i)}>✕</button></div>
                {/each}
                <button on:click={() => addLocalUrl(srv.id, "uk")}>+ UK ссылка</button>
              </div>
              <div>
                <h3>PDD</h3>
                {#each links.forum_pdd_urls as _, i}
                  <div class="input-row"><input class="input" bind:value={cfg.servers[String(srv.id)].forum_pdd_urls[i]} placeholder="https://forum..." /><button on:click={() => removeLocalUrl(srv.id, "pdd", i)}>✕</button></div>
                {/each}
                <button on:click={() => addLocalUrl(srv.id, "pdd")}>+ PDD ссылка</button>
              </div>
            </div>
          </section>
        {/each}
        <button class="primary" on:click={save}>Сохранить локальные ссылки</button>
      {/if}
    {/if}

    {#if activeTab === "config"}
      <section class="title-row"><div><p class="eyebrow">Локальные настройки</p><h1>Конфиг</h1></div><button class="primary" on:click={save}>Сохранить</button></section>
      <section class="panel"><h2>Backend</h2><label>URL backend<input class="input" bind:value={cfg.ai.backend_url} placeholder="https://api.smart.moder42.tech" /></label><button on:click={loadBackend}>Проверить подключение</button>{#if backendError}<p class="error-text">{backendError}</p>{/if}</section>
      <section class="panel"><h2>Папка вывода</h2><div class="input-row"><input class="input" bind:value={cfg.output_dir} placeholder="Папка вывода" /><button on:click={pickOutputDir}>📂</button><button on:click={() => openShell(cfg.output_dir || configPath.replace(/[^/]+$/, ""))}>↗</button></div></section>
      <section class="grid-2">
        <div class="panel"><h2>Arizona форум</h2><label>Логин<input class="input" bind:value={cfg.arizona.login} /></label><label>Пароль<div class="input-row">{#if showPw.arizona}<input class="input" type="text" bind:value={cfg.arizona.password} />{:else}<input class="input" type="password" bind:value={cfg.arizona.password} />{/if}<button on:click={() => showPw.arizona = !showPw.arizona}>👁</button></div></label></div>
        <div class="panel"><h2>Rodina форум</h2><label>Логин<input class="input" bind:value={cfg.rodina.login} /></label><label>Пароль<div class="input-row">{#if showPw.rodina}<input class="input" type="text" bind:value={cfg.rodina.password} />{:else}<input class="input" type="password" bind:value={cfg.rodina.password} />{/if}<button on:click={() => showPw.rodina = !showPw.rodina}>👁</button></div></label></div>
      </section>
    {/if}

    {#if activeTab === "ai"}
      <section class="title-row"><div><p class="eyebrow">Провайдер</p><h1>AI настройки</h1></div><button class="primary" on:click={save}>Сохранить</button></section>
      <section class="panel"><div class="segmented"><button class:selected={cfg.ai.provider === "gemini"} on:click={() => cfg.ai.provider = "gemini"}>Gemini локально</button><button class:selected={cfg.ai.provider === "openai"} on:click={() => cfg.ai.provider = "openai"}>OpenAI через backend</button></div></section>
      <section class="grid-2">
        <div class="panel"><h2>Gemini локально</h2><p class="muted">При Gemini лимиты не списываются.</p><label>Модель<input class="input" bind:value={cfg.ai.gemini_model} /></label>{#each cfg.ai.gemini_api_keys as _, i}<label>Ключ #{i+1}<div class="input-row"><input class="input" type="password" bind:value={cfg.ai.gemini_api_keys[i]} /><button on:click={() => { cfg.ai.gemini_api_keys.splice(i, 1); cfg = cfg; }}>✕</button></div></label>{/each}<button on:click={() => cfg.ai.gemini_api_keys = [...cfg.ai.gemini_api_keys, ""]}>+ ключ</button></div>
        <div class="panel"><h2>OpenAI</h2><p>OpenAI-ключ хранится только на backend. В приложении сохраняется только Telegram/backend token.</p><label>Модель<input class="input" bind:value={cfg.ai.openai_model} /></label></div>
      </section>
    {/if}

    {#if activeTab === "profile"}
      <section class="title-row"><div><p class="eyebrow">Аккаунт</p><h1>Профиль</h1></div>{#if backendUser}<button class="danger" on:click={logout}>Выйти</button>{/if}</section>
      {#if backendUser}
        <section class="panel"><p><b>Telegram ID:</b> {backendUser.telegram_id}</p><p><b>Роль:</b> {backendUser.role}</p><p><b>Лимит:</b> {backendUser.used_today ?? 0}/{backendUser.daily_limit}</p></section>
      {:else}
        <section class="panel"><p>Ты работаешь локально. Для командной истории, лимитов и OpenAI войди через Telegram-код.</p><div class="input-row"><input class="input" bind:value={loginCode} placeholder="Код из бота" /><button class="primary" on:click={loginWithTelegramCode}>Войти</button></div></section>
      {/if}
    {/if}
  </main>
</div>
