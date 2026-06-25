"""Generate placeholder PNG and ICO icons for What Changed?"""
import struct
import zlib

def create_png_rgba(width, height, r=99, g=102, b=241, a=255):
    """Create a minimal solid-color RGBA PNG."""
    def chunk(chunk_type, data):
        c = chunk_type + data
        crc = struct.pack(">I", zlib.crc32(c) & 0xffffffff)
        return struct.pack(">I", len(data)) + c + crc

    signature = b"\x89PNG\r\n\x1a\n"
    # bit_depth=8, color_type=6 (RGBA)
    ihdr = chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, 6, 0, 0, 0))

    raw = b""
    for y in range(height):
        raw += b"\x00"  # filter none
        for x in range(width):
            raw += bytes([r, g, b, a])

    idat = chunk(b"IDAT", zlib.compress(raw))
    iend = chunk(b"IEND", b"")
    return signature + ihdr + idat + iend


def create_png_rgb(width, height, r=99, g=102, b=241):
    """Create a minimal solid-color RGB PNG."""
    def chunk(chunk_type, data):
        c = chunk_type + data
        crc = struct.pack(">I", zlib.crc32(c) & 0xffffffff)
        return struct.pack(">I", len(data)) + c + crc

    signature = b"\x89PNG\r\n\x1a\n"
    # bit_depth=8, color_type=2 (RGB)
    ihdr = chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, 2, 0, 0, 0))

    raw = b""
    for y in range(height):
        raw += b"\x00"  # filter none
        for x in range(width):
            raw += bytes([r, g, b])

    idat = chunk(b"IDAT", zlib.compress(raw))
    iend = chunk(b"IEND", b"")
    return signature + ihdr + idat + iend


def create_ico(width, height, r=99, g=102, b=241):
    """Create a minimal ICO file with one image."""
    header = struct.pack("<HHH", 0, 1, 1)  # reserved, type=1(ICO), count=1

    row_size = (width * 3 + 3) & ~3
    mask_row_size = (width + 31) // 32 * 4
    image_size = 40 + row_size * height + mask_row_size * height

    bmp_header = struct.pack("<IiiHHIIiiII",
        40, width, height * 2, 1, 24, 0, image_size, 0, 0, 0, 0
    )

    pixels = b""
    for y in range(height - 1, -1, -1):
        for x in range(width):
            pixels += bytes([b, g, r])

    and_mask = b"\x00" * mask_row_size * height
    image_data = bmp_header + pixels + and_mask

    dir_entry = struct.pack("<BBBBHHII",
        min(width, 255), min(height, 255), 0, 0, 1, 24,
        len(image_data), 6 + 16
    )

    return header + dir_entry + image_data


if __name__ == "__main__":
    import os
    icons_dir = r"C:\VScode\what-changed\src-tauri\icons"
    os.makedirs(icons_dir, exist_ok=True)

    # Regular PNGs (RGB - for Tauri app icons)
    for size, name in [(32, "32x32.png"), (128, "128x128.png"), (256, "128x128@2x.png")]:
        with open(os.path.join(icons_dir, name), "wb") as f:
            f.write(create_png_rgb(size, size))
        print(f"Created {name} (RGB)")

    # ICO
    with open(os.path.join(icons_dir, "icon.ico"), "wb") as f:
        f.write(create_ico(256, 256))
    print("Created icon.ico")

    # Tray icon - RGBA format (required by Tauri for system tray)
    with open(os.path.join(icons_dir, "icon.png"), "wb") as f:
        f.write(create_png_rgba(32, 32))
    print("Created icon.png (RGBA)")

    print("All icons generated!")
