"""
disasm_all.py - disassemble every extracted D3D9 shader (.vso/.pso) with the Windows SDK's fxc.exe
(`fxc /dumpbin` disassembles SM2/SM3 bytecode incl. the CTAB constant table, i.e. register names).

Usage: py disasm_all.py [shader_dir ...]   (default: every local/shaders/<zone>/)
Output: <shader_dir>/asm/<name>.vso.asm / .pso.asm  (game-derived, git-ignored)
If fxc.exe is not found, falls back to d3d9_disasm.py (minimal pure-Python disassembler).
"""
import os, sys, glob, subprocess
HERE = os.path.dirname(os.path.abspath(__file__))


def find_fxc():
    c = sorted(glob.glob(r'C:\Program Files (x86)\Windows Kits\10\bin\10.*\x64\fxc.exe'))
    return c[-1] if c else None


def main():
    dirs = sys.argv[1:] or sorted(glob.glob(os.path.join(HERE, 'local', 'shaders', '*')))
    fxc = find_fxc()
    for d in dirs:
        if not os.path.isdir(d): continue
        out = os.path.join(d, 'asm'); os.makedirs(out, exist_ok=True)
        n = 0
        for sub in ('vs', 'ps'):
            for f in sorted(os.listdir(os.path.join(d, sub))):
                src = os.path.join(d, sub, f); dst = os.path.join(out, f + '.asm')
                if os.path.exists(dst): continue
                if fxc:
                    subprocess.run([fxc, '/nologo', '/dumpbin', '/Fc', dst, src], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
                if not os.path.exists(dst):
                    import d3d9_disasm
                    open(dst, 'w').write(d3d9_disasm.disassemble(open(src, 'rb').read()))
                n += 1
        print(d, n, 'new listings')


if __name__ == '__main__':
    sys.path.insert(0, HERE)
    main()
