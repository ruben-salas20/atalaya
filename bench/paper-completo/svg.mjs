// Convierte cada línea (LaTeX, con entidades HTML) a SVG con MathJax y mide.
import fs from 'fs';
import {mathjax} from 'mathjax-full/js/mathjax.js';
import {TeX} from 'mathjax-full/js/input/tex.js';
import {SVG} from 'mathjax-full/js/output/svg.js';
import {liteAdaptor} from 'mathjax-full/js/adaptors/liteAdaptor.js';
import {RegisterHTMLHandler} from 'mathjax-full/js/handlers/html.js';
import {AllPackages} from 'mathjax-full/js/input/tex/AllPackages.js';
const adaptor = liteAdaptor(); RegisterHTMLHandler(adaptor);
const doc = mathjax.document('', {InputJax: new TeX({packages: AllPackages}), OutputJax: new SVG({fontCache: 'none'})});
const unesc = s => s.replace(/&lt;/g,'<').replace(/&gt;/g,'>').replace(/&quot;/g,'"').replace(/&#39;/g,"'").replace(/&amp;/g,'&');
for (const f of process.argv.slice(2)) {
  const lines = fs.readFileSync(f, 'utf8').split('\n').filter(Boolean);
  let errors = [], bytes = 0, t0 = performance.now(), sample = '';
  lines.forEach((l, i) => {
    const node = doc.convert(unesc(l), {display: false});
    const svg = adaptor.innerHTML(node);
    if (svg.includes('data-mjx-error') || svg.includes('merror')) errors.push(unesc(l).slice(0, 80));
    bytes += svg.length;
    if (i === 3) sample = svg.slice(0, 160);
  });
  const ms = performance.now() - t0;
  console.log(JSON.stringify({f, formulas: lines.length, errores: errors.length, ejemplos_error: errors.slice(0, 3), kb_total: Math.round(bytes/1024), bytes_medio: Math.round(bytes/lines.length), ms_total: Math.round(ms), muestra: sample}));
}
