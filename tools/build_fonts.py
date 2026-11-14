"""Builds the UI fonts in app/res/fonts: static Regular + SemiBold Latin subsets.

Download the files below from https://github.com/google/fonts (ofl/<dir>/, with
OFL.txt -> app/res/fonts/OFL-<Name>.txt; check it has no "Reserved Font Name"),
then: python -I tools/build_fonts.py <download dir> app/res/fonts app/res/fonts
(args: download dir, output dir, dir with the Atkinson / JetBrains subsets whose
characters set the subset). Needs fonttools (pip install fonttools).
"""
import sys, os
from fontTools.ttLib import TTFont
from fontTools.varLib import instancer
from fontTools import subset

src, out, ref = sys.argv[1], sys.argv[2], sys.argv[3]
codes = set()
for f in ['AtkinsonHyperlegibleNext-Regular.ttf', 'JetBrainsMono-Regular.ttf']:
    codes |= set(TTFont(os.path.join(ref, f)).getBestCmap())
codes -= {0xFEFF}
codes |= {0x2022}  # bullet

FONTS = [  # (output name, file under the download dir, axes for Regular, axes for SemiBold)
    ('Inter', 'inter/Inter[opsz,wght].ttf', {'wght': 400, 'opsz': 14}, {'wght': 600, 'opsz': 14}),
    ('Roboto', 'roboto/Roboto[wdth,wght].ttf', {'wght': 400, 'wdth': 100}, {'wght': 600, 'wdth': 100}),
    ('OpenSans', 'opensans/OpenSans[wdth,wght].ttf', {'wght': 400, 'wdth': 100}, {'wght': 600, 'wdth': 100}),
    ('NotoSans', 'notosans/NotoSans[wdth,wght].ttf', {'wght': 400, 'wdth': 100}, {'wght': 600, 'wdth': 100}),
    ('FiraSans', ('firasans/FiraSans-Regular.ttf', 'firasans/FiraSans-SemiBold.ttf'), None, None),
    ('Figtree', 'figtree/Figtree[wght].ttf', {'wght': 400}, {'wght': 600}),
]

def make(font, path):
    opts = subset.Options()
    opts.layout_features = ['kern']
    opts.hinting = False
    opts.desubroutinize = True
    opts.name_IDs = ['*']
    opts.notdef_outline = True
    s = subset.Subsetter(opts)
    s.populate(unicodes=codes)
    s.subset(font)
    font.save(path)
    print(path, os.path.getsize(path))

for name, f, reg, semi in FONTS:
    for weight, axes, i in [('Regular', reg, 0), ('SemiBold', semi, 1)]:
        if axes is None:
            font = TTFont(os.path.join(src, f[i]))
        else:
            font = instancer.instantiateVariableFont(TTFont(os.path.join(src, f)), axes, updateFontNames=False)
            # Pin any axis not named to its default (makes the font static).
            if 'fvar' in font:
                font = instancer.instantiateVariableFont(font, {a.axisTag: None for a in font['fvar'].axes})
        make(font, os.path.join(out, f'{name}-{weight}.ttf'))
