"""Offline audit helper; validated against EIP-1014 vectors and saved RPC hashes.
Not production signing or hot-path code.
"""
# minimal Keccak-256
RC=[1,0x8082,0x800000000000808a,0x8000000080008000,0x808b,0x80000001,0x8000000080008081,0x8000000000008009,0x8a,0x88,0x80008009,0x8000000a,0x8000808b,0x800000000000008b,0x8000000000008089,0x8000000000008003,0x8000000000008002,0x8000000000000080,0x800a,0x800000008000000a,0x8000000080008081,0x8000000000008080,0x80000001,0x8000000080008008]
R=[[0,36,3,41,18],[1,44,10,45,2],[62,6,43,15,61],[28,55,25,21,56],[27,20,39,8,14]]
MASK=(1<<64)-1
def rol(x,n): return ((x<<n)|(x>>(64-n)))&MASK if n else x
def keccak_f(a):
 for rc in RC:
  c=[a[x]^a[x+5]^a[x+10]^a[x+15]^a[x+20] for x in range(5)]
  d=[c[(x-1)%5]^rol(c[(x+1)%5],1) for x in range(5)]
  for x in range(5):
   for y in range(5): a[x+5*y]^=d[x]
  b=[0]*25
  for x in range(5):
   for y in range(5): b[y+5*((2*x+3*y)%5)]=rol(a[x+5*y],R[x][y])
  for x in range(5):
   for y in range(5): a[x+5*y]=b[x+5*y]^((~b[(x+1)%5+5*y])&b[(x+2)%5+5*y])
  a[0]^=rc
 return a
def k256(data):
 rate=136; p=bytearray(data); p.append(1)
 while len(p)%rate != rate-1: p.append(0)
 p.append(0x80); a=[0]*25
 for off in range(0,len(p),rate):
  block=p[off:off+rate]
  for i in range(rate//8): a[i]^=int.from_bytes(block[8*i:8*i+8],'little')
  keccak_f(a)
 return b''.join(x.to_bytes(8,'little') for x in a)[:32]
