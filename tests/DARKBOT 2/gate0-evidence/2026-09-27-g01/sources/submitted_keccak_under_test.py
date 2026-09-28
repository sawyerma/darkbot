RC = [
0x0000000000000001,0x0000000000008082,0x800000000000808A,0x8000000080008000,
0x000000000000808B,0x0000000080000001,0x8000000080008081,0x8000000000008009,
0x000000000000008A,0x0000000000000088,0x0000000080008009,0x000000008000000A,
0x000000008000808B,0x800000000000008B,0x8000000000008089,0x8000000000008003,
0x8000000000008002,0x8000000000000080,0x000000000000800A,0x800000008000000A,
0x8000000080008081,0x8000000000008080,0x0000000080000001,0x8000000080008008]
ROT = [
[0,36,3,41,18],
[1,44,10,45,2],
[62,6,43,15,61],
[28,55,25,21,56],
[27,20,39,8,14]
]
MASK=(1<<64)-1
def rol(x,n): return ((x<<n)|(x>>(64-n)))&MASK if n else x
def keccak_f(st):
    for rc in RC:
        C=[st[x]^st[x+5]^st[x+10]^st[x+15]^st[x+20] for x in range(5)]
        D=[C[(x-1)%5]^rol(C[(x+1)%5],1) for x in range(5)]
        for x in range(5):
            for y in range(5):
                st[x+5*y]^=D[x]
        B=[0]*25
        for x in range(5):
            for y in range(5):
                B[y+5*((2*x+3*y)%5)] = rol(st[x+5*y], ROT[x][y])
        for x in range(5):
            for y in range(5):
                st[x+5*y]=B[x+5*y]^((~B[(x+1)%5+5*y])&B[(x+2)%5+5*y])
        st[0]^=rc
    return st
def keccak256(data:bytes):
    rate=136
    padded=bytearray(data)
    padded.append(0x01)
    while len(padded)%rate != rate-1:
        padded.append(0)
    padded.append(0x80)
    st=[0]*25
    for off in range(0,len(padded),rate):
        block=padded[off:off+rate]
        for i in range(rate//8):
            st[i]^=int.from_bytes(block[8*i:8*i+8],"little")
        keccak_f(st)
    out=bytearray()
    while len(out)<32:
        for i in range(rate//8):
            out += st[i].to_bytes(8,"little")
            if len(out)>=32: return bytes(out[:32])
        keccak_f(st)
def selector(sig): return "0x"+keccak256(sig.encode()).hex()[:8]
