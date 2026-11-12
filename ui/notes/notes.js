const sidebar = document.getElementById('sidebar');
const titleInput = document.getElementById('note-title');
const bodyInput = document.getElementById('note-body');
const emptyState = document.getElementById('empty-state');
const saveStatus = document.getElementById('save-status');

let allNotes = [];
let activeId = null;
let saveTimer = null;

function genId() {
  return Date.now().toString(36) + Math.random().toString(36).slice(2, 7);
}

function formatDate(iso) {
  try {
    const d = new Date(iso);
    return d.toLocaleDateString('fr-FR', { day: 'numeric', month: 'short' });
  } catch { return ''; }
}

function showEditor(show) {
  titleInput.style.display = show ? '' : 'none';
  bodyInput.style.display = show ? '' : 'none';
  emptyState.style.display = show ? 'none' : '';
}

function renderSidebar() {
  sidebar.innerHTML = '';
  const sorted = [...allNotes].sort((a, b) =>
    new Date(b.updated) - new Date(a.updated));

  for (const note of sorted) {
    const item = document.createElement('button');
    item.className = 'note-item' + (note.id === activeId ? ' active' : '');

    const title = document.createElement('span');
    title.className = 'note-title';
    title.textContent = note.title || 'Sans titre';
    item.appendChild(title);

    const date = document.createElement('span');
    date.className = 'note-date';
    date.textContent = formatDate(note.updated);
    item.appendChild(date);

    const del = document.createElement('button');
    del.className = 'note-del';
    del.textContent = '×';
    del.title = 'Supprimer';
    del.addEventListener('click', (e) => {
      e.stopPropagation();
      deleteNote(note.id);
    });
    item.appendChild(del);

    item.addEventListener('click', () => selectNote(note.id));
    sidebar.appendChild(item);
  }
}

function selectNote(id) {
  flushSave();
  activeId = id;
  const note = allNotes.find((n) => n.id === id);
  if (note) {
    titleInput.value = note.title || '';
    bodyInput.value = note.body || '';
    showEditor(true);
  } else {
    showEditor(false);
  }
  renderSidebar();
}

function createNote() {
  flushSave();
  const note = {
    id: genId(),
    title: '',
    body: '',
    created: new Date().toISOString(),
    updated: new Date().toISOString(),
  };
  allNotes.unshift(note);
  activeId = note.id;
  showEditor(true);
  titleInput.value = '';
  bodyInput.value = '';
  titleInput.focus();
  renderSidebar();
  persistNow();
}

function deleteNote(id) {
  allNotes = allNotes.filter((n) => n.id !== id);
  if (activeId === id) {
    activeId = allNotes.length ? allNotes[0].id : null;
    if (activeId) selectNote(activeId);
    else showEditor(false);
  }
  renderSidebar();
  persistNow();
}

function markDirty() {
  saveStatus.textContent = 'Non enregistre';
  saveStatus.className = 'saving';
  clearTimeout(saveTimer);
  saveTimer = setTimeout(persistNow, 800);
}

function flushSave() {
  if (saveTimer) {
    clearTimeout(saveTimer);
    saveTimer = null;
    syncActive();
    notes.save(allNotes);
  }
}

function syncActive() {
  if (!activeId) return;
  const note = allNotes.find((n) => n.id === activeId);
  if (!note) return;
  // la date de modification ne change que si le contenu a changé
  if (note.title === titleInput.value && note.body === bodyInput.value) return;
  note.title = titleInput.value;
  note.body = bodyInput.value;
  note.updated = new Date().toISOString();
}

async function persistNow() {
  clearTimeout(saveTimer);
  saveTimer = null;
  syncActive();
  await notes.save(allNotes);
  // titres et dates à jour dans la liste (sans perdre le focus de l'éditeur)
  renderSidebar();
  saveStatus.textContent = 'Enregistre';
  saveStatus.className = 'saved';
  setTimeout(() => {
    if (saveStatus.className === 'saved') {
      saveStatus.textContent = '';
      saveStatus.className = '';
    }
  }, 2000);
}

titleInput.addEventListener('input', () => {
  markDirty();
  syncActive(); // le titre de la liste suit la saisie
  renderSidebar();
});
bodyInput.addEventListener('input', markDirty);

document.getElementById('btn-new').addEventListener('click', createNote);
document.getElementById('btn-close').addEventListener('click', () => {
  flushSave();
  overlay.hide();
});

document.getElementById('btn-bold').addEventListener('click', () => {
  wrapSelection('**', '**');
});
document.getElementById('btn-italic').addEventListener('click', () => {
  wrapSelection('_', '_');
});
document.getElementById('btn-list').addEventListener('click', () => {
  const start = bodyInput.selectionStart;
  const lineStart = bodyInput.value.lastIndexOf('\n', start - 1) + 1;
  bodyInput.setRangeText('- ', lineStart, lineStart);
  bodyInput.focus();
  markDirty();
});

function wrapSelection(before, after) {
  const start = bodyInput.selectionStart;
  const end = bodyInput.selectionEnd;
  const text = bodyInput.value.substring(start, end);
  bodyInput.setRangeText(before + text + after, start, end);
  bodyInput.selectionStart = start + before.length;
  bodyInput.selectionEnd = start + before.length + text.length;
  bodyInput.focus();
  markDirty();
}

window.addEventListener('keydown', (e) => {
  if (e.key === 'Escape') {
    flushSave();
    overlay.hide();
  }
  if (e.ctrlKey && !e.shiftKey && e.key.toLowerCase() === 'n') {
    e.preventDefault();
    createNote();
  }
});

overlay.onShown(() => titleInput.focus());

notes.load().then((loaded) => {
  allNotes = Array.isArray(loaded) ? loaded : [];
  if (allNotes.length) {
    const sorted = [...allNotes].sort((a, b) =>
      new Date(b.updated) - new Date(a.updated));
    selectNote(sorted[0].id);
  } else {
    showEditor(false);
  }
  renderSidebar();
});

// ---------- transparence du fond (mémorisée) ----------
(() => {
  const op = document.getElementById('op-slider');
  const apply = (v) => { document.documentElement.style.setProperty('--a', v); op.value = v; };
  overlay.getOpacity().then((v) => apply(v ?? 0.92));
  let t = null;
  op.addEventListener('input', () => {
    apply(op.value);
    clearTimeout(t);
    t = setTimeout(() => overlay.saveOpacity(Number(op.value)), 300);
  });
})();
