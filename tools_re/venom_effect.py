import copy


def decode(blob):
    count = blob[0]
    header = blob[:count + 1]
    pos = len(header)
    parts = []
    for _ in range(count):
        loaded, mapped = blob[pos:pos + 2]
        pos += 2
        parts.append((blob[pos:pos + loaded], blob[pos + loaded:pos + loaded + mapped]))
        pos += loaded + mapped
    count = blob[pos]
    pos += 1
    sequences = []
    for _ in range(count):
        frames = []
        frame_count = blob[pos]
        pos += 1
        for _ in range(frame_count):
            directions = []
            for _ in range(4):
                size = blob[pos]
                pos += 1
                directions.append([list(blob[k:k + 4]) for k in range(pos, pos + size * 4, 4)])
                pos += size * 4
            frames.append(directions)
        sequences.append(frames)
    if pos != len(blob):
        raise ValueError('Unexpected hero animation data')
    return header, parts, sequences


def encode(header, parts, sequences):
    out = bytearray(header)
    for loaded, mapped in parts:
        out.extend((len(loaded), len(mapped)))
        out.extend(loaded)
        out.extend(mapped)
    out.append(len(sequences))
    for frames in sequences:
        out.append(len(frames))
        for directions in frames:
            for sprites in directions:
                out.append(len(sprites))
                for sprite in sprites:
                    out.extend(sprite)
    return bytes(out)


def poison_motes(frame, casting=False):
    sprites = []
    for i, x in enumerate((-14, -9, 5, 12)):
        y = 3 - ((frame * 3 + i * 7) % 30)
        sprites.append([4, 20, x & 255, y & 255])
        sprites.append([4, 21, (x + 3) & 255, (y - 5) & 255])
    if casting:
        for x in (-10, -3, 4, 11):
            sprites.append([4, 20, x & 255, (5 - frame) & 255])
    return sprites


def patch(blob):
    header, parts, sequences = decode(blob)
    if len(sequences) != 17 or len(sequences[12]) != 6 or len(sequences[10]) != 8 or len(sequences[14]) != 8:
        raise ValueError('Expected original rogue animations')
    loaded, mapped = parts[4]
    parts[4] = (loaded + bytes(i for i in (20, 21) if i not in loaded), mapped)
    cast = copy.deepcopy([sequences[10][i] for i in (0, 1, 2, 3, 5, 7)])
    for f, directions in enumerate(cast):
        for d, sprites in enumerate(directions):
            directions[d] = [s for s in sprites if s[0] & 127 != 4] + poison_motes(f, True)
    sequences[12] = cast
    sequences.append([[poison_motes(f), [], [], []] for f in range(8)])
    return encode(header, parts, sequences)
