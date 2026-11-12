// Mini rendu Markdown (sûr : tout le texte est échappé avant d'être mis en forme).
(() => {
  const escape = (s) =>
    s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;');

  function inline(text) {
    const codes = [];
    let s = escape(text).replace(/`([^`]+)`/g, (_m, c) => `\u0000${codes.push(c) - 1}\u0000`);
    s = s
      .replace(/\*\*(.+?)\*\*/g, '<strong>$1</strong>')
      .replace(/(^|[^*])\*([^*\s][^*]*?)\*/g, '$1<em>$2</em>')
      .replace(/\[([^\]]+)\]\((https?:\/\/[^\s)]+)\)/g, '<a href="$2" target="_blank">$1</a>');
    return s.replace(/\u0000(\d+)\u0000/g, (_m, i) => `<code>${codes[i]}</code>`);
  }

  // Formules : extraites avant tout traitement, rendues en MathML (KaTeX), réinsérées à la fin.
  const MARK = '\u0001';
  let maths = [];
  function stashMath(src) {
    maths = [];
    const put = (tex, display) => {
      let html = null;
      try { if (window.katex) html = katex.renderToString(tex, { displayMode: display, output: 'mathml', throwOnError: false, strict: false }); } catch {}
      return MARK + (maths.push(html || `<code>${escape(tex)}</code>`) - 1) + MARK;
    };
    return src
      .replace(/\$\$([\s\S]+?)\$\$/g, (_m, t) => put(t.trim(), true))
      .replace(/\\\[([\s\S]+?)\\\]/g, (_m, t) => put(t.trim(), true))
      .replace(/\\\((.+?)\\\)/g, (_m, t) => put(t, false))
      .replace(/\$(?=\S)([^$\n]+?)(?<=\S)\$(?!\d)/g, (_m, t) => put(t, false));
  }
  const unstash = (html) => html.replace(/\u0001(\d+)\u0001/g, (_m, i) => maths[i]);

  function render(src) {
    src = stashMath(src);
    const out = [];
    const lines = src.replace(/\r/g, '').split('\n');
    let i = 0;
    while (i < lines.length) {
      const line = lines[i];

      const fence = line.match(/^\s*```(\w*)/);
      if (fence) {
        const code = [];
        i++;
        while (i < lines.length && !/^\s*```/.test(lines[i])) code.push(lines[i++]);
        i++;
        out.push(
          `<div class="code"><div class="code-bar"><span>${escape(fence[1] || 'code')}</span>` +
            `<button class="copy">Copier</button></div><pre><code>${escape(code.join('\n'))}</code></pre></div>`
        );
        continue;
      }

      const heading = line.match(/^(#{1,4})\s+(.*)/);
      if (heading) {
        const level = Math.min(heading[1].length + 2, 6);
        out.push(`<h${level}>${inline(heading[2])}</h${level}>`);
        i++;
        continue;
      }

      const listMatch = line.match(/^\s*([-*•]|\d+[.)])\s+/);
      if (listMatch) {
        const ordered = /\d/.test(listMatch[1]);
        const items = [];
        while (i < lines.length && /^\s*([-*•]|\d+[.)])\s+/.test(lines[i])) {
          items.push(`<li>${inline(lines[i].replace(/^\s*([-*•]|\d+[.)])\s+/, ''))}</li>`);
          i++;
        }
        const tag = ordered ? 'ol' : 'ul';
        out.push(`<${tag}>${items.join('')}</${tag}>`);
        continue;
      }

      if (!line.trim()) {
        i++;
        continue;
      }

      const para = [];
      while (
        i < lines.length &&
        lines[i].trim() &&
        !/^\s*```/.test(lines[i]) &&
        !/^#{1,4}\s/.test(lines[i]) &&
        !/^\s*([-*•]|\d+[.)])\s+/.test(lines[i])
      ) {
        para.push(inline(lines[i++]));
      }
      out.push(`<p>${para.join('<br>')}</p>`);
    }
    return unstash(out.join(''));
  }

  window.renderMarkdown = render;
})();
