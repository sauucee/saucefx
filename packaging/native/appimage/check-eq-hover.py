"""Check the released 0.3.3 band-centered hover guide at the 1040x568 test viewport."""
from pathlib import Path
import sys
from PIL import Image, ImageChops


def verify(directory):
    # Only sample the uniform upper half of the chart in this flat EQ fixture.
    # Exclude the knob and the readout, which are allowed to change on hover.
    region = (650, 358, 760, 424)
    def chart(name):
        with Image.open(directory / f'native-{name}.png') as image:
            assert image.size == (1040, 568), image.size
            return image.convert('RGB').crop(region)
    idle = chart('idle')
    def ink(name):
        return ImageChops.difference(idle, chart(name)).getbbox()
    left, right, adjacent = (ink(n) for n in ('left', 'right', 'adjacent'))
    assert left is not None and left == right, ('guide moved within the same band', left, right)
    for box, center in ((left, 693.5), (adjacent, 715.0)):
        assert box and 1 <= box[2] - box[0] <= 2 and (box[1], box[3]) == (0, idle.height), box
        assert abs(region[0] + (box[0] + box[2]) / 2 - center) <= 1.5, (box, center)
    assert ink('leave') is None, 'guide stayed after leaving the EQ'
    print('Guide stayed on 1 kHz, moved to the adjacent band, and cleared on exit.')


if __name__ == '__main__':
    verify(Path(sys.argv[1]))
