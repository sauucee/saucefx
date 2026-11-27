"""Compare the rendered stroke between handles, and save unaltered crops for visual review."""
import json
import math
import sys
from pathlib import Path
from PIL import Image, ImageDraw, ImageFilter, ImageFont

out = Path(sys.argv[1])
cases = ('Music', 'Movies', 'Single-band')
font = ImageFont.truetype('DejaVuSans.ttf', 18)
sheet = Image.new('RGB', (1340, 3*231+42), '#111317')
labels = ImageDraw.Draw(sheet)
labels.text((12, 8), 'Original SauceFX 0.2.4', font=font, fill='white')
labels.text((676, 8), 'Native SauceFX candidate', font=font, fill='white')


def stroke(image):
    return [tuple(y for y in range(image.height)
                  if (lambda c: c[0] > 95 and c[0]-c[1] > 42 and c[0]-c[2] > 18)(image.getpixel((x,y))))
            for x in range(image.width)]


for row, case in enumerate(cases):
    rect = json.loads((out / f'{case}-chart.json').read_text())
    x, y, w, h = (round(rect[k]) for k in ('x','y','width','height'))
    crops = []
    for col, variant in enumerate(('original','native')):
        with Image.open(out / f'{case}-{variant}.png') as full:
            assert full.size == (1040,568), full.size
            chart = full.convert('RGB').crop((x,y,x+w,y+h))
        crops.append(chart)
        labels.text((12+664*col, 42+231*row), case, font=font, fill='#c3c9d2')
        sheet.paste(chart, (12+664*col, 70+231*row))
    original, native = map(stroke, crops)
    # Exclude handles; an equal row of dots must never pass as a smooth line.
    columns = [i for i in range(math.ceil(w/62), math.floor(w-w/62))
               if min(abs(i-(k+.5)*w/31) for k in range(31)) > 6]
    if case == 'Single-band':
        columns = [i for i in columns if abs(i-(17.5*w/31)) < 2*w/31]
    valid = [i for i in columns if original[i] and native[i]]
    assert len(valid) >= .9*len(columns) and len(valid) >= 15, (case,'missing stroke',len(valid),len(columns))
    for src, dest in ((original,native),(native,original)):
        # Symmetric distance in screen pixels handles steep segments better than
        # comparing their y coordinates at only one x. No rescaling or hand-fit.
        count = matched = 0
        for i in columns:
            for sy in src[i]:
                count += 1
                matched += any(abs(dy-sy) <= 2 for j in range(i-2,i+3) for dy in dest[j])
        assert count > 0 and matched/count >= .98, (case,'stroke mismatch',matched,count)
        print(f'{case}: {matched}/{count} between-handle stroke pixels within 2 px', flush=True)
sheet.save(out / 'eq-comparison.png')
