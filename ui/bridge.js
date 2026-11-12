// Pont entre les pages et Orbiz (Rust) : remplace les preloads Electron (mêmes objets window.orbiz, overlay, chat, notes, browserOverlay).
(() => {
  const T = window.__TAURI__;
  const invoke = T.core.invoke;
  const on = (ev, cb) => T.event.listen(ev, (e) => cb(e.payload));
  const page = location.pathname.split('/').filter(Boolean)[0] || '';

  if (page === 'orb') {
    window.orbiz = {
      getActions: () => invoke('orb_actions'),
      runAction: (id) => invoke('orb_run_action', { id }),
      setIgnore: (ignore) => invoke('orb_set_ignore', { ignore }),
      dragStart: () => invoke('orb_drag_start'),
      dragEnd: () => invoke('orb_drag_end'),
      menuOpened: () => invoke('orb_menu_open'),
      menuClosed: () => invoke('orb_menu_close'),
      contextMenu: () => invoke('orb_context_menu'),
      // position de la souris dans la fenêtre, même quand les clics la traversent
      onCursor: (cb) => on('cursor', cb),
      onMotion: (cb) => on('motion', cb),
      onForceClose: (cb) => on('force-close', cb),
      onQuitAnimation: (cb) => on('quit-animation', cb),
      quitReady: () => invoke('orb_quit_ready'),
    };
    return;
  }

  window.overlay = {
    hide: () => invoke('overlay_hide'),
    onShown: (cb) => on('overlay:shown', cb),
    // transparence de toute la fenêtre (mini navigateur), mémorisée
    setOpacity: (value) => invoke('overlay_set_opacity', { value }),
    // transparence du fond gérée par la page (chat, notes) : seulement mémorisée
    saveOpacity: (value) => invoke('overlay_save_opacity', { value }),
    getOpacity: () => invoke('overlay_get_opacity'),
  };
  window.chat = {
    getSettings: () => invoke('chat_get_settings'),
    saveSettings: (s) => invoke('chat_save_settings', { s }),
    listModels: () => invoke('chat_list_models'),
    getUsage: () => invoke('chat_get_usage'),
    resetUsage: () => invoke('chat_reset_usage'),
    send: (id, text, from) => invoke('chat_send', { id, text, from: from ?? null }),
    stop: (id) => invoke('chat_stop', { id }),
    newChat: () => invoke('chat_new'),
    onDelta: (cb) => on('chat:delta', cb),
    onDone: (cb) => on('chat:done', cb),
    onError: (cb) => on('chat:error', cb),
    onStatus: (cb) => on('chat:status', cb),
    onExternal: (cb) => on('chat:external', cb),
  };
  window.notes = {
    load: () => invoke('notes_load'),
    save: (data) => invoke('notes_save', { data }),
  };
  window.browserOverlay = {
    getPending: () => invoke('browser_get_pending'),
    onNavigate: (cb) => on('browser:navigate', cb),
    onUrl: (cb) => on('browser:url', cb),
    onTitle: (cb) => on('browser:title', cb),
    view: (url, r) => invoke('browser_view', { url: url ?? null, x: r.x, y: r.y, w: r.w, h: r.h }),
    nav: (action) => invoke('browser_nav', { action }),
  };

  // Déplacer la fenêtre en tirant une zone `-webkit-app-region: drag` (les boutons et champs restent cliquables).
  const region = (el) => {
    for (let e = el; e && e !== document.documentElement; e = e.parentElement) {
      if (/^(button|input|textarea|select|a)$/i.test(e.tagName)) return 'no-drag';
      const v = getComputedStyle(e).getPropertyValue('-webkit-app-region') || getComputedStyle(e).getPropertyValue('app-region');
      if (v === 'drag' || v === 'no-drag') return v;
    }
    // repli si WebView2 n'expose pas app-region : barres de titre connues
    return el.closest && el.closest('header, #chrome, #nav') ? 'drag' : '';
  };
  document.addEventListener('mousedown', (e) => {
    if (e.button !== 0 || region(e.target) !== 'drag') return;
    e.preventDefault();
    T.window.getCurrentWindow().startDragging();
  });
  // Échap / F12 hors champs de saisie
  window.addEventListener('keydown', (e) => {
    if (e.key === 'F5') { e.preventDefault(); location.reload(); }
  }, true);
})();
