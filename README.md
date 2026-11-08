# wayland pointer position (wlpp)

A wayland client that prints out the pointer position.

## Usage

```
Usage: wlpp [OPTION]

A wayland client that prints out the pointer position as JSON.

Options:
  --watch   Continuously print the pointer position
  --help    Print help
```

Example output:
```json
{"output": {"name": "DP-3", "description": "Media4 Inc - Q27 PRO - DP-3"}, "x": 532, "y": 806}
```

## Dependencies
- `cargo` or `rustup`
- `libxkbcommon`
- wayland compositor with `ext-image-capture-source-v1` and `ext-image-copy-capture-v1` support.
