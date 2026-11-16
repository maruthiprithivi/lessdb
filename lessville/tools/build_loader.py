import base64, io, os, re
from pathlib import Path
from PIL import Image
root = Path(os.environ.get('LESSVILLE_ROOT', Path(__file__).resolve().parents[1]))
p = root / 'web/loading.html'
t = p.read_text()
im = Image.open(root / 'assets/share/favicon.png').resize((128, 128), Image.NEAREST)
buf = io.BytesIO(); im.save(buf, 'PNG', optimize=True)
b64 = base64.b64encode(buf.getvalue()).decode()

art = '''  <style id="loading-art">
    html, body { background: #0f131b; color: #e8edf5; }
    #loading { box-sizing: border-box; place-content: center; gap: 14px; background: #0f131b; text-align: center; }
    #loading-icon { width: 112px; height: 112px; justify-self: center; image-rendering: pixelated; }
    #loading h1 { font-weight: 700; font-size: clamp(26px, 4vw, 40px); letter-spacing: 0.08em; color: #f2b84b; }
    #loading-tagline { color: #9aa6b8; font-size: 15px; }
    #loading-status { font-weight: 500; }
    #loading progress { display: block; width: min(320px, 75vw); height: 7px; border: 0; border-radius: 20px; overflow: hidden; accent-color: #f2b84b; background: #ffffff1f; }
    #loading progress::-webkit-progress-bar { background: #ffffff1f; border-radius: 20px; }
    #loading progress::-webkit-progress-value { background: #f2b84b; border-radius: 20px; }
    #loading progress::-moz-progress-bar { background: #f2b84b; border-radius: 20px; }
    #loading button { min-height: 44px; border: 1px solid #f2b84b; border-radius: 10px; background: #161c27; color: #e8edf5; }
    #loading button:focus-visible { outline: 3px solid #f2b84b; outline-offset: 4px; }
    #loading [hidden] { display: none; }
    @media (prefers-reduced-motion: reduce) { #loading * { transition: none !important; animation: none !important; } }
  </style>
'''
t = re.sub(r'  <style id="loading-art">.*?</style>\n', lambda m: art, t, count=1, flags=re.S)
t = t.replace('<section id="loading" role="status">\n    <h1 id="game-title"></h1>',
              '<section id="loading" role="status">\n    <img id="loading-icon" alt="" src="data:image/png;base64,%s">\n    <h1 id="game-title"></h1>\n    <p id="loading-tagline">A LessDB agent world</p>' % b64, 1)
t = re.sub(r"const zh = String\(.*?\);", "const zh = false; // Single-language game: pinned to English.", t, count=1, flags=re.S)
if '<link rel="icon"' not in t:
    t = t.replace('<title>$GODOT_PROJECT_NAME</title>', '<title>$GODOT_PROJECT_NAME</title>\n  <link rel="icon" type="image/png" href="data:image/png;base64,%s">' % b64, 1)
p.write_text(t)
print('ok', len(b64), 'zh pinned' if 'const zh = false' in t else 'zh NOT pinned', t.count('loading-icon'))
