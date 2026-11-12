const orb = document.getElementById('orb');
const squash = document.getElementById('squash');
const ring = document.getElementById('ring');
const label = document.getElementById('label');
const mouth = document.getElementById('mouth');
const particles = document.getElementById('particles');
const RING_RADIUS = 95;
const DEFAULT_LABEL = 'Orbiz';

const eyeL = document.getElementById('eye-l');
const eyeR = document.getElementById('eye-r');
const irisL = eyeL.querySelector('.iris');
const irisR = eyeR.querySelector('.iris');
const cheekL = document.getElementById('cheek-l');
const cheekR = document.getElementById('cheek-r');

let actions = [];
let open = false;
let dragging = false;
let down = null;
let ignoring = true;

// ── mouse state ──
let mX = 0, mY = 0, mIn = false;
let lastActivity = performance.now();

// ── expression state ──
const MOOD_IDLE = 'idle';
const MOOD_ATTENTIVE = 'attentive';
const MOOD_HAPPY = 'happy';
const MOOD_SURPRISED = 'surprised';
const MOOD_SLEEP = 'sleep';
let mood = MOOD_IDLE;
let moodStart = 0;

// ── blink ──
let nextBlink = 3000 + Math.random() * 4000;
let blinkUntil = 0;

// ── sleep ──
const SLEEP_DELAY = 45000;
let sleeping = false;
let zzzEl = null;

// ── idle curiosity ──
let curiosityTarget = null;
let nextCuriosity = 8000 + Math.random() * 6000;

// ── click-through ──
function setIgnore(v) {
  if (v === ignoring) return;
  ignoring = v;
  orbiz.setIgnore(v);
}

// Rust envoie la position de la souris dans la fenêtre (même quand les clics la traversent) :
// on regarde si elle survole une zone .hit pour capter ou laisser passer les clics.
orbiz.onCursor(({ x, y, inside }) => {
  mX = x;
  mY = y;
  mIn = inside;
  if (inside) lastActivity = performance.now();
  if (dragging) return;
  const el = inside ? document.elementFromPoint(x, y) : null;
  setIgnore(!(el && el.closest('.hit')));
});

// ── drag / click ──
orb.addEventListener('pointerdown', (e) => {
  if (e.button !== 0 || open) return;
  dragging = true;
  down = { x: e.screenX, y: e.screenY, t: performance.now() };
  orb.setPointerCapture(e.pointerId);
  orb.classList.add('grab');
  setIgnore(false);
  orbiz.dragStart();
  wakeUp();
  setMood(MOOD_SURPRISED);
});

function endDrag(e) {
  if (!dragging) return;
  dragging = false;
  orb.classList.remove('grab');
  orbiz.dragEnd();
  const moved = Math.hypot(e.screenX - down.x, e.screenY - down.y);
  if (e.type === 'pointerup' && moved < 5 && performance.now() - down.t < 400) {
    setMood(MOOD_HAPPY);
    openMenu();
  } else {
    setTimeout(() => setMood(MOOD_IDLE), 600);
  }
}
orb.addEventListener('pointerup', endDrag);
orb.addEventListener('pointercancel', endDrag);

orb.addEventListener('contextmenu', (e) => {
  e.preventDefault();
  orbiz.contextMenu();
});

// ── menu ──
function openMenu() {
  if (open) return;
  open = true;
  label.textContent = DEFAULT_LABEL;
  orb.classList.add('open');
  orbiz.menuOpened();
}

function closeMenu() {
  if (!open) return;
  open = false;
  orb.classList.remove('open');
  orbiz.menuClosed();
  setTimeout(() => setMood(MOOD_IDLE), 300);
}

function trigger(action) {
  orbiz.runAction(action.id);
  orb.classList.add('pulse');
  setTimeout(() => {
    orb.classList.remove('pulse');
    closeMenu();
  }, 140);
}

document.getElementById('center').addEventListener('click', closeMenu);

window.addEventListener('keydown', (e) => {
  if (!open) return;
  if (e.key === 'Escape') closeMenu();
  const n = Number(e.key);
  if (n >= 1 && n <= actions.length) trigger(actions[n - 1]);
});

orbiz.onForceClose(closeMenu);

function buildRing() {
  ring.innerHTML = '';
  actions.forEach((action, i) => {
    const angle = -Math.PI / 2 + (i * 2 * Math.PI) / actions.length;
    const x = Math.cos(angle) * RING_RADIUS;
    const y = Math.sin(angle) * RING_RADIUS;

    const btn = document.createElement('button');
    btn.className = 'action';
    btn.title = action.label;
    btn.innerHTML = window.ICONS[action.icon] || '';
    btn.style.setProperty('--x', `${x}px`);
    btn.style.setProperty('--y', `${y}px`);
    btn.style.setProperty('--d', `${0.04 + i * 0.05}s`);
    btn.addEventListener('mouseenter', () => (label.textContent = action.label));
    btn.addEventListener('mouseleave', () => (label.textContent = DEFAULT_LABEL));
    btn.addEventListener('click', () => trigger(action));
    ring.appendChild(btn);

    const lbl = document.createElement('span');
    lbl.className = 'act-label';
    const lblDist = RING_RADIUS + 38;
    lbl.style.left = `calc(50% + ${Math.cos(angle) * lblDist}px)`;
    lbl.style.top = `calc(50% + ${Math.sin(angle) * lblDist}px)`;
    lbl.style.transform = 'translate(-50%, -50%)';
    lbl.textContent = action.label;
    ring.appendChild(lbl);
  });
}

orbiz.getActions().then((list) => {
  actions = list;
  buildRing();
});

// ── mood system ──
function setMood(m) {
  if (mood === m) return;
  mood = m;
  moodStart = performance.now();
}

function wakeUp() {
  if (sleeping) {
    sleeping = false;
    if (zzzEl) { zzzEl.remove(); zzzEl = null; }
    eyeL.classList.remove('sleep');
    eyeR.classList.remove('sleep');
    mouth.className = 'mouth';
    setMood(MOOD_SURPRISED);
    setTimeout(() => { if (mood === MOOD_SURPRISED) setMood(MOOD_HAPPY); }, 500);
    setTimeout(() => { if (mood === MOOD_HAPPY) setMood(MOOD_IDLE); }, 1400);
  }
  lastActivity = performance.now();
}

function goToSleep() {
  if (sleeping || open || dragging) return;
  sleeping = true;
  setMood(MOOD_SLEEP);
  eyeL.classList.add('sleep');
  eyeR.classList.add('sleep');
  mouth.className = 'mouth sleep';
  cheekL.classList.remove('blush');
  cheekR.classList.remove('blush');
  curiosityTarget = null;

  if (!zzzEl) {
    zzzEl = document.createElement('div');
    zzzEl.className = 'zzz';
    zzzEl.innerHTML = '<span>z</span><span>z</span><span>z</span>';
    orb.appendChild(zzzEl);
  }
}

// ── sparkle particles ──
function spawnParticles() {
  if (particles.children.length > 5) return;
  const el = document.createElement('div');
  el.className = 'spark';
  const angle = Math.random() * Math.PI * 2;
  const startR = 35 + Math.random() * 15;
  el.style.left = `${50 + Math.cos(angle) * startR}%`;
  el.style.top = `${50 + Math.sin(angle) * startR}%`;
  el.style.setProperty('--dx', `${(Math.random() - .5) * 20}px`);
  el.style.setProperty('--dy', `${-15 - Math.random() * 20}px`);
  el.style.setProperty('--dur', `${2.5 + Math.random() * 2}s`);
  el.style.setProperty('--delay', '0s');
  particles.appendChild(el);
  setTimeout(() => el.remove(), 5000);
}

// ── eye tracking ──
function updateEyes(t) {
  if (sleeping || open) return;

  const cx = window.innerWidth / 2;
  const cy = window.innerHeight / 2;
  const dist = mIn ? Math.hypot(mX - cx, mY - cy) : 999;
  const proximity = Math.max(0, 1 - dist / 160);

  // blink
  if (t > nextBlink && !eyeL.classList.contains('blink')) {
    eyeL.classList.add('blink');
    eyeR.classList.add('blink');
    blinkUntil = t + 100;
    nextBlink = t + 2500 + Math.random() * 5000;
  }
  if (t > blinkUntil && eyeL.classList.contains('blink')) {
    eyeL.classList.remove('blink');
    eyeR.classList.remove('blink');
    eyeL.classList.add('blink-open');
    eyeR.classList.add('blink-open');
    setTimeout(() => {
      eyeL.classList.remove('blink-open');
      eyeR.classList.remove('blink-open');
    }, 150);
  }

  // expression-based eye state
  eyeL.classList.remove('wide', 'happy');
  eyeR.classList.remove('wide', 'happy');

  if (mood === MOOD_SURPRISED) {
    eyeL.classList.add('wide');
    eyeR.classList.add('wide');
  } else if (mood === MOOD_HAPPY) {
    eyeL.classList.add('happy');
    eyeR.classList.add('happy');
  }

  // iris tracking
  let lookX = 0, lookY = 0;

  if (curiosityTarget && !mIn) {
    lookX = curiosityTarget.x;
    lookY = curiosityTarget.y;
  } else if (mIn) {
    const a = Math.atan2(mY - cy, mX - cx);
    const strength = Math.min(proximity * 1.2, 1);
    lookX = Math.cos(a) * strength * 3.5;
    lookY = Math.sin(a) * strength * 3.5;
  }

  irisL.style.transform = `translate(${lookX}px, ${lookY}px)`;
  irisR.style.transform = `translate(${lookX}px, ${lookY}px)`;
}

// ── mouth expressions ──
function updateMouth(t) {
  if (sleeping || open) return;

  const cx = window.innerWidth / 2;
  const cy = window.innerHeight / 2;
  const dist = mIn ? Math.hypot(mX - cx, mY - cy) : 999;
  const proximity = Math.max(0, 1 - dist / 160);

  if (mood === MOOD_SURPRISED) {
    mouth.className = 'mouth surprise';
  } else if (mood === MOOD_HAPPY) {
    mouth.className = 'mouth grin';
  } else if (proximity > 0.4) {
    mouth.className = 'mouth smile';
  } else {
    mouth.className = 'mouth smile';
  }
}

// ── cheeks ──
function updateCheeks() {
  if (sleeping || open) {
    cheekL.classList.remove('blush');
    cheekR.classList.remove('blush');
    return;
  }

  const should = mood === MOOD_HAPPY || mood === MOOD_SURPRISED || dragging;
  cheekL.classList.toggle('blush', should);
  cheekR.classList.toggle('blush', should);
}

// ── idle curiosity (random look-around) ──
function updateCuriosity(t) {
  if (sleeping || open || dragging || mIn) {
    curiosityTarget = null;
    return;
  }

  if (t > nextCuriosity) {
    const a = Math.random() * Math.PI * 2;
    const r = 1.5 + Math.random() * 2;
    curiosityTarget = { x: Math.cos(a) * r, y: Math.sin(a) * r };
    nextCuriosity = t + 6000 + Math.random() * 8000;
    setTimeout(() => { curiosityTarget = null; }, 1500 + Math.random() * 1000);
  }
}

// ── squash & stretch + floating ──
let target = { s: 0, a: 0 };
let current = { s: 0, a: 0 };

orbiz.onMotion(({ vx, vy }) => {
  const speed = Math.hypot(vx, vy);
  target.s = open ? 0 : Math.min(speed / 50, 0.2);
  if (speed > 0.5) target.a = Math.atan2(vy, vx);
});

// ── main animation loop ──
let particleTimer = 0;

(function animate() {
  const t = performance.now();

  // sleep check
  if (!sleeping && !open && !dragging && t - lastActivity > SLEEP_DELAY) {
    goToSleep();
  }

  // proximity-based wake
  if (sleeping && mIn) {
    const cx = window.innerWidth / 2;
    const cy = window.innerHeight / 2;
    const dist = Math.hypot(mX - cx, mY - cy);
    if (dist < 120) wakeUp();
  }

  // auto-expire moods
  const moodAge = t - moodStart;
  if (mood === MOOD_SURPRISED && moodAge > 600) setMood(MOOD_HAPPY);
  if (mood === MOOD_HAPPY && moodAge > 2000) setMood(MOOD_IDLE);

  // idle micro-float
  const idle = !open && !dragging;
  const floatX = idle ? Math.sin(t / 3500 + 1) * 1.3 : 0;
  const floatY = idle ? Math.sin(t / 2600) * 2.2 : 0;

  // squash & stretch
  current.s += (target.s - current.s) * 0.2;
  let da = target.a - current.a;
  da = Math.atan2(Math.sin(da), Math.cos(da));
  current.a += da * 0.3;
  const s = current.s;
  squash.style.transform =
    `translate(${floatX}px,${floatY}px) rotate(${current.a}rad) scale(${1 + s}, ${1 - s * 0.6}) rotate(${-current.a}rad)`;

  // subsystems
  updateEyes(t);
  updateMouth(t);
  updateCheeks();
  updateCuriosity(t);

  // particles (spawn every ~2.5s when idle, less when active)
  if (t - particleTimer > (idle ? 2500 : 5000)) {
    particleTimer = t;
    if (!open) spawnParticles();
  }

  requestAnimationFrame(animate);
})();

// ── quit animation ──
orbiz.onQuitAnimation(() => {
  if (open) closeMenu();
  if (sleeping) wakeUp();

  // burst of sparkles
  for (let i = 0; i < 12; i++) {
    setTimeout(() => spawnParticles(), i * 80);
  }

  // phase 1: surprise → happy
  setMood(MOOD_SURPRISED);
  cheekL.classList.add('blush');
  cheekR.classList.add('blush');

  setTimeout(() => {
    setMood(MOOD_HAPPY);
    mouth.className = 'mouth grin';
  }, 400);

  // phase 2: eyes close peacefully, smile stays
  setTimeout(() => {
    eyeL.classList.remove('wide', 'happy');
    eyeR.classList.remove('wide', 'happy');
    eyeL.classList.add('sleep');
    eyeR.classList.add('sleep');
    mouth.className = 'mouth smile';
  }, 1000);

  // phase 3: shrink + spin + fade
  setTimeout(() => {
    orb.style.transition = 'transform 1s cubic-bezier(.4,0,.2,1), opacity 1s ease';
    orb.style.transform = 'scale(0) rotate(360deg)';
    orb.style.opacity = '0';
  }, 1400);

  // done
  setTimeout(() => {
    orbiz.quitReady();
  }, 2300);
});
