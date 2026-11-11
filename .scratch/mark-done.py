import pathlib, re, sys
for name in sys.argv[1:]:
    p = pathlib.Path('.scratch/system-sentinel/issues') / name
    s = p.read_text()
    s = s.replace('**Status:** ready-for-agent', '**Status:** done')
    s = re.sub(r'- \[ \]', '- [x]', s)
    p.write_text(s)
    print('done', name)
