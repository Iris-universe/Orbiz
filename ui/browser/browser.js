const frame = document.getElementById('view-container');
const rect = () => { const r = frame.getBoundingClientRect(); return { x: r.left, y: r.top, w: r.width, h: r.height }; };
let created = false;
const place = (url) => { created = true; return browserOverlay.view(url, rect()); };
new ResizeObserver(() => { if (created) place(); }).observe(frame);
const urlBar = document.getElementById('url-bar');
const backBtn = document.getElementById('back-btn');
const fwdBtn = document.getElementById('fwd-btn');
const goBtn = document.getElementById('go-btn');
const closeBtn = document.getElementById('close-btn');
const opacitySlider = document.getElementById('opacity-slider');

function navigate(url) {
  if (!url) return;
  if (!/^https?:\/\//i.test(url)) {
    if (/^[a-z0-9.-]+\.[a-z]{2,}/i.test(url.split('/')[0])) {
      url = 'https://' + url;
    } else {
      url = 'https://www.google.com/search?q=' + encodeURIComponent(url);
    }
  }
  place(url);
  urlBar.value = url;
}

urlBar.addEventListener('keydown', (e) => {
  if (e.key === 'Enter') {
    navigate(urlBar.value.trim());
    urlBar.blur();
  }
  if (e.key === 'Escape') urlBar.blur();
});

goBtn.addEventListener('click', () => navigate(urlBar.value.trim()));
backBtn.addEventListener('click', () => browserOverlay.nav('back'));
fwdBtn.addEventListener('click', () => browserOverlay.nav('forward'));
closeBtn.addEventListener('click', () => overlay.hide());
overlay.getOpacity().then((v) => { if (v != null) opacitySlider.value = Math.round(v * 100); });
opacitySlider.addEventListener('input', () => {
  overlay.setOpacity(parseInt(opacitySlider.value, 10) / 100);
});

browserOverlay.onUrl((url) => { if (document.activeElement !== urlBar) urlBar.value = url; });
browserOverlay.onTitle((t) => { document.title = t || 'Iris Mini'; });

// receive URLs from IrisBrowser
browserOverlay.onNavigate((url) => navigate(url));

overlay.onShown(async () => {
  const pending = await browserOverlay.getPending();
  if (pending) navigate(pending);
  else if (!created) place('about:blank');
  urlBar.focus();
});

// premier affichage : l'évènement « shown » part parfois avant que la page écoute
browserOverlay.getPending().then((pending) => {
  if (pending) navigate(pending);
  else if (!created) place('about:blank');
});
