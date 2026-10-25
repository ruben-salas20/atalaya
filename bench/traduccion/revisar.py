#!/usr/bin/env python3
"""Revisión automática de un resultado de medir.py.

Uso: python3 revisar.py resultados/<archivo>.json [más archivos]

Por cada texto comprueba:
- HTML: que la traducción tiene las mismas etiquetas que el original, y los mismos href.
- Marcas coladas: etiquetas que no estaban en el original, como <translation>.
- Inglés sin traducir: párrafos donde abundan palabras muy comunes del inglés.
- Repeticiones: la misma palabra dos veces seguidas («despidos y despidos» no lo cuenta; «de de», sí).
Con varios archivos del mismo modelo, cuenta además cuántos textos cambian entre pasadas.
"""
import json
import re
import sys
from collections import Counter
from pathlib import Path

import corpus

HERE = Path(__file__).resolve().parent
TAG = re.compile(r'</?([a-zA-Z][a-zA-Z0-9]*)\b[^>]*>')
HREF = re.compile(r'href="([^"]*)"')
# Palabras que en un texto español casi nunca aparecen.
ENGLISH = {'the', 'and', 'of', 'to', 'is', 'that', 'with', 'for', 'this', 'are', 'was', 'have', 'it', 'which', 'from'}
REPEAT = re.compile(r'\b(\w{2,})\s+\1\b', re.IGNORECASE)


def english_share(text):
    words = re.findall(r'[a-zA-Z]+', TAG.sub(' ', text).lower())
    if len(words) < 8:
        return 0.0
    return sum(w in ENGLISH for w in words) / len(words)


def paragraphs(html):
    return [p for p in re.split(r'</p>\s*', html) if p.strip()]


def review(path, originals):
    data = json.loads(Path(path).read_text())
    problems = []
    for r in data['textos']:
        if 'traduccion' not in r or 'texto' not in originals[r['id']]:
            continue  # noticia sin texto en el repo público: ver corpus.py
        orig = originals[r['id']]['texto']
        tr = r['traduccion'] or ''
        notes = []
        if originals[r['id']].get('formato') == 'html':
            to, tt = Counter(TAG.findall(orig)), Counter(TAG.findall(tr))
            if to != tt:
                missing = to - tt
                extra = tt - to
                if missing:
                    notes.append('faltan etiquetas %s' % dict(missing))
                if extra:
                    notes.append('etiquetas de más %s' % dict(extra))
            if HREF.findall(orig) != HREF.findall(tr):
                notes.append('href distintos')
            english = [i + 1 for i, p in enumerate(paragraphs(tr)) if english_share(p) > 0.12]
            if english:
                notes.append('párrafos en inglés: %s' % english)
        else:
            extra = set(TAG.findall(tr))
            if extra:
                notes.append('marcas coladas %s' % sorted(extra))
            if english_share(tr) > 0.12:
                notes.append('parece inglés')
        reps = [m.group(0) for m in REPEAT.finditer(TAG.sub(' ', tr))]
        if reps:
            notes.append('repeticiones %s' % reps)
        if r.get('cortados'):
            notes.append('%d segmentos cortados' % r['cortados'])
        if notes:
            problems.append((r['id'], notes))
    return data, problems


def main():
    originals = {t['id']: t for t in json.loads(corpus.default_path().read_text())['textos']}
    runs = []
    for path in sys.argv[1:]:
        data, problems = review(path, originals)
        runs.append(data)
        print('== %s · %s' % (data['modelo'], Path(path).name))
        print('   %d de %d textos con avisos' % (len(problems), len(data['textos'])))
        skipped = sum(1 for r in data['textos'] if 'traduccion' not in r)
        if skipped:
            print('   %d sin revisar: no guardan la traducción (ver corpus.py)' % skipped)
        for tid, notes in problems:
            print('   %-12s %s' % (tid, '; '.join(notes)))
    if len(runs) > 1:
        ids = [t['id'] for t in runs[0]['textos']]
        changed = [i for i in ids
                   if len({next(t.get('traduccion') for t in run['textos'] if t['id'] == i) for run in runs}) > 1]
        print('== Textos que cambian entre las %d pasadas: %d de %d' % (len(runs), len(changed), len(ids)))


if __name__ == '__main__':
    main()
