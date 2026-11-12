const $ = (id) => document.getElementById(id);
const panel = $('panel');
const list = $('messages');
const empty = $('empty');
const input = $('input');
const settings = $('settings');

const escapeHtml = (s) => s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');

let current = null; // réponse en cours : { id, el, text }
let toolActions = [];
let config = { hasKey: false, model: null, opacity: 0.6 };

// ---------- affichage ----------

function applyConfig() {
  document.documentElement.style.setProperty('--a', config.opacity);
  $('op-slider').value = config.opacity;
  $('model-name').textContent = config.model || '';
}

function scrollDown() {
  list.scrollTop = list.scrollHeight;
}

function addBubble(role, text) {
  empty.hidden = true;
  const el = document.createElement('div');
  el.className = `msg ${role}`;
  if (role === 'assistant') el.innerHTML = renderMarkdown(text);
  else el.textContent = text;
  list.appendChild(el);
  scrollDown();
  return el;
}

function autoGrow() {
  input.style.height = 'auto';
  input.style.height = `${input.scrollHeight}px`;
}

function setBusy(busy) {
  document.body.classList.toggle('busy', busy);
  $('btn-send').title = busy ? 'Arrêter' : 'Envoyer';
}

function toolLabel(tool, args) {
  switch (tool) {
    case 'navigate': return `Navigation vers ${args.url || '?'}`;
    case 'get_page_content': return 'Lecture du contenu';
    case 'list_interactive_elements': return 'Analyse des éléments';
    case 'click': return `Clic sur ${args.text || args.selector || (args.index != null ? '#' + args.index : '?')}`;
    case 'type_text': return `Saisie de texte`;
    case 'select_option': return `Sélection : ${args.value || '?'}`;
    case 'scroll': return `Défilement ${args.direction === 'up' ? '↑' : args.direction === 'down' ? '↓' : args.direction || ''}`;
    case 'go_back': return 'Page précédente';
    case 'show_browser': return 'Affichage du navigateur';
    case 'hide_browser': return 'Masquage du navigateur';
    case 'execute_javascript': return 'Exécution JS';
    case 'modify_page': return 'Modification de la page';
    case 'press_key': return `Touche ${args.key || '?'}`;
    case 'wait': return `Attente ${args.seconds || '?'}s`;
    default: {
      const i = tool.indexOf('__');
      const nice = (s) => s.replace(/_/g, ' ');
      if (tool.startsWith('app_')) return `${args.app || 'app'} · ${nice(tool.slice(4))}`;
      return i > 0 ? `${tool.slice(0, i)} · ${nice(tool.slice(i + 2))}` : tool;
    }
  }
}

function updateBubble() {
  if (!current) return;
  let html = '';
  if (toolActions.length > 0) {
    html += '<div class="tool-log">' +
      toolActions.map((a) => `<div>${escapeHtml(a)}</div>`).join('') +
      '</div>';
  }
  if (current.text) {
    html += renderMarkdown(current.text);
  }
  current.el.innerHTML = html;
  const atBottom = list.scrollHeight - list.scrollTop - list.clientHeight < 40;
  if (atBottom) scrollDown();
}

// ---------- conversation ----------

function send(from) {
  const text = input.value.trim();
  if (!text || current) return;
  if (!config.hasKey) return openSettings();

  input.value = '';
  autoGrow();
  const ub = addBubble('user', text);
  if (typeof from === 'string') ub.dataset.from = from;

  const id = `${Date.now()}-${Math.random().toString(36).slice(2)}`;
  const el = addBubble('assistant', '');
  el.classList.add('typing');
  current = { id, el, text: '' };
  toolActions = [];
  setBusy(true);
  chat.send(id, text, typeof from === 'string' ? from : undefined);
}

function finish() {
  if (!current) return;
  current.el.classList.remove('typing', 'working');
  if (current.text || toolActions.length > 0) {
    updateBubble();
  } else {
    current.el.remove();
  }
  current = null;
  toolActions = [];
  setBusy(false);
  input.focus();
}

chat.onDelta(({ id, text }) => {
  if (current?.id !== id) return;
  current.el.classList.add('typing');
  current.text += text;
  updateBubble();
});

chat.onStatus(({ id, tool, args }) => {
  if (current?.id !== id) return;
  current.el.classList.remove('typing');
  current.el.classList.add('working');
  toolActions.push(toolLabel(tool, args));
  updateBubble();
});

chat.onDone(({ id, cost }) => {
  if (current?.id !== id) return;
  const el = current.el;
  finish();
  if (cost != null && cost > 0 && el.isConnected) {
    const c = document.createElement('div');
    c.className = 'msg-cost';
    c.textContent = usd(cost, true);
    c.title = 'Coût estimé de cette réponse (outils compris)';
    el.appendChild(c);
  }
});

chat.onError(({ id, message }) => {
  if (current?.id !== id) return;
  finish();
  addBubble('error', message);
});

function newChat() {
  if (current) chat.stop(current.id);
  chat.newChat();
  list.querySelectorAll('.msg').forEach((el) => el.remove());
  empty.hidden = false;
  current = null;
  toolActions = [];
  setBusy(false);
  input.focus();
}

// ---------- réglages ----------

// ---------- coûts ----------

const usd = (v, small = false) => {
  if (v == null) return '?';
  const digits = v === 0 ? 2 : small || v < 0.1 ? (v < 0.01 ? 4 : 3) : 2;
  return v.toLocaleString('fr-FR', { minimumFractionDigits: digits, maximumFractionDigits: digits }) + ' $';
};
const rate = (v) => v.toLocaleString('fr-FR', { maximumFractionDigits: 3 }) + ' $';

let modelPrices = {};

function showModelPrice() {
  const p = modelPrices[$('set-model').value];
  const el = $('model-price');
  if (!$('set-model').value) { el.textContent = ''; return; }
  if (!p) { el.textContent = 'Prix inconnu pour ce modèle : ses dépenses ne seront pas chiffrées.'; return; }
  const cache = p.cached < p.input ? ` · cache <b>${rate(p.cached)}</b>` : '';
  el.innerHTML = `Par million de tokens : entrée <b>${rate(p.input)}</b>${cache} · sortie <b>${rate(p.output)}</b>`;
}

async function fillUsage() {
  try {
    const u = await chat.getUsage();
    $('u-today').textContent = usd(u.today);
    $('u-month').textContent = usd(u.month);
    $('u-total').textContent = usd(u.total);
    $('u-total-label').textContent = u.since
      ? `depuis le ${new Date(u.since).toLocaleDateString('fr-FR')}` : 'total';
    const rows = Object.entries(u.byModel).sort((a, b) => b[1].cost - a[1].cost);
    $('u-models').innerHTML = rows.map(([model, m]) =>
      `<li><span>${escapeHtml(model)} · ${m.requests} requête${m.requests > 1 ? 's' : ''}</span><span>${
        m.unpriced === m.requests ? 'prix inconnu' : usd(m.cost) + (m.unpriced ? ' + ?' : '')}</span></li>`
    ).join('');
  } catch {}
}

async function fillModels() {
  const select = $('set-model');
  const keep = config.model;
  select.innerHTML = '';
  if (!config.hasKey) {
    select.add(new Option('Enregistre une clé d’abord', ''));
    return showModelPrice();
  }
  try {
    const models = await chat.listModels();
    modelPrices = {};
    for (const { id, price } of models) {
      modelPrices[id] = price;
      const label = price ? `${id}  —  ${rate(price.input)} / ${rate(price.output)}` : `${id}  —  prix inconnu`;
      select.add(new Option(label, id, false, id === keep));
    }
  } catch (err) {
    if (keep) select.add(new Option(keep, keep, true, true));
    if (config.hasKey) $('settings-error').textContent = err.message;
  }
  showModelPrice();
}

function openSettings() {
  $('settings-error').textContent = '';
  $('set-key').value = '';
  $('key-status').textContent = config.hasKey
    ? 'Une clé est enregistrée (chiffrée). Laisse vide pour la garder.'
    : 'Ta clé est stockée chiffrée sur ce PC.';
  $('set-opacity').value = config.opacity;
  $('set-limit').value = config.costLimit ?? 0.15;
  settings.hidden = false;
  fillModels();
  fillUsage();
  (config.hasKey ? $('set-model') : $('set-key')).focus();
}

function closeSettings() {
  settings.hidden = true;
  input.focus();
}

async function saveSettings() {
  const btn = $('btn-save');
  btn.disabled = true;
  $('settings-error').textContent = '';
  try {
    const key = $('set-key').value.trim();
    const model = key ? null : $('set-model').value || null;
    const limitRaw = $('set-limit').value.trim().replace(',', '.');
    config = await chat.saveSettings({
      key: key || null, model, opacity: Number($('set-opacity').value),
      costLimit: limitRaw === '' ? null : Number(limitRaw),
    });
    applyConfig();
    if (key) {
      $('set-key').value = '';
      $('key-status').textContent = 'Clé enregistrée ✓';
      await fillModels();
    } else {
      closeSettings();
    }
  } catch (err) {
    $('settings-error').textContent = String(err.message).replace(/^Error invoking remote method '[^']+': (Error: )?/, '');
  } finally {
    btn.disabled = false;
  }
}

$('set-opacity').addEventListener('input', (e) => {
  document.documentElement.style.setProperty('--a', e.target.value);
  $('op-slider').value = e.target.value;
});
// curseur de la barre du haut : même réglage, enregistré directement
let opTimer = null;
$('op-slider').addEventListener('input', (e) => {
  const v = Number(e.target.value);
  document.documentElement.style.setProperty('--a', v);
  $('set-opacity').value = v;
  config.opacity = v;
  clearTimeout(opTimer);
  opTimer = setTimeout(() => chat.saveSettings({ opacity: v }).then((c) => { config = { ...config, ...c }; }).catch(() => {}), 400);
});
$('set-opacity').addEventListener('pointerdown', () => settings.classList.add('peek'));
window.addEventListener('pointerup', () => settings.classList.remove('peek'));

// ---------- événements ----------

$('composer').addEventListener('submit', (e) => {
  e.preventDefault();
  if (current) chat.stop(current.id);
  else send();
});

input.addEventListener('input', autoGrow);
input.addEventListener('keydown', (e) => {
  if (e.key === 'Enter' && !e.shiftKey) {
    e.preventDefault();
    send();
  }
});

$('btn-new').addEventListener('click', newChat);
$('btn-settings').addEventListener('click', () => (settings.hidden ? openSettings() : closeSettings()));
$('btn-close').addEventListener('click', () => overlay.hide());
$('btn-save').addEventListener('click', saveSettings);
$('set-model').addEventListener('change', showModelPrice);
$('btn-reset-usage').addEventListener('click', async () => {
  if (!confirm('Remettre le compteur de dépenses à zéro ?')) return;
  await chat.resetUsage();
  fillUsage();
});
$('set-key').addEventListener('keydown', (e) => e.key === 'Enter' && saveSettings());

window.addEventListener('keydown', (e) => {
  if (e.key !== 'Escape') return;
  if (!settings.hidden) {
    applyConfig();
    closeSettings();
  } else overlay.hide();
});

list.addEventListener('click', (e) => {
  const btn = e.target.closest('.copy');
  if (!btn) return;
  navigator.clipboard.writeText(btn.closest('.code').querySelector('code').textContent);
  btn.textContent = 'Copié ✓';
  setTimeout(() => (btn.textContent = 'Copier'), 1200);
});

overlay.onShown(() => {
  panel.style.animation = 'none';
  void panel.offsetWidth;
  panel.style.animation = '';
  if (settings.hidden) input.focus();
});

// Question envoyée depuis une app (iris-link)
chat.onExternal(({ text, send: auto, from }) => {
  if (current && auto) chat.stop(current.id);
  input.value = text;
  autoGrow();
  input.focus();
  input.setSelectionRange(text.length, text.length);
  if (auto) {
    const go = () => (current ? setTimeout(go, 150) : send(from));
    go();
  }
});

chat.getSettings().then((s) => {
  config = s;
  applyConfig();
  if (!config.hasKey) openSettings();
});
