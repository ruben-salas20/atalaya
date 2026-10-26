// Convierte fórmulas LaTeX en SVG con MathJax (ADR 0010). Lo lanza el
// servicio una vez por paper: lee de la entrada estándar una lista JSON
// [{"latex": "...", "display": false}, ...] y escribe otra con, por fórmula,
// {"svg", "vertical_align", "width", "height"} o null si MathJax no la entiende.
import {mathjax} from 'mathjax-full/js/mathjax.js';
import {TeX} from 'mathjax-full/js/input/tex.js';
import {SVG} from 'mathjax-full/js/output/svg.js';
import {liteAdaptor} from 'mathjax-full/js/adaptors/liteAdaptor.js';
import {RegisterHTMLHandler} from 'mathjax-full/js/handlers/html.js';
import {AllPackages} from 'mathjax-full/js/input/tex/AllPackages.js';

const adaptor = liteAdaptor();
RegisterHTMLHandler(adaptor);
// Sin `noundefined` y con `formatError` que lanza: una macro desconocida
// (como las `\lx@…` de LaTeXML) da null en vez de pintarse en rojo.
const doc = mathjax.document('', {
  InputJax: new TeX({
    packages: AllPackages.filter((p) => p !== 'noundefined'),
    formatError: (_jax, err) => {
      throw err;
    },
  }),
  OutputJax: new SVG({fontCache: 'none'}),
});

let input = '';
for await (const chunk of process.stdin) input += chunk;
const out = JSON.parse(input).map(({latex, display}) => {
  try {
    const node = doc.convert(latex, {display: Boolean(display)});
    const svg = adaptor.firstChild(node);
    const html = adaptor.innerHTML(node);
    if (html.includes('data-mjx-error') || html.includes('merror')) return null;
    const style = adaptor.getAttribute(svg, 'style') || '';
    const va = /vertical-align:\s*([-0-9.]+ex)/.exec(style);
    return {
      svg: html,
      vertical_align: va ? va[1] : '0ex',
      width: adaptor.getAttribute(svg, 'width'),
      height: adaptor.getAttribute(svg, 'height'),
    };
  } catch {
    return null;
  }
});
process.stdout.write(JSON.stringify(out));
