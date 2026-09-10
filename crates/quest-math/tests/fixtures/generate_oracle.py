"""Independent Decimal full-matrix goldens; no quest-math code is imported.

Pi uses Gauss-Legendre AGM, independently of the verifier's Machin formula.
Gate matrices multiply densely as Decimal complex pairs. Each residual is checked
at 100 and 140 decimal digits before committing a 50-digit enclosure fixture.
Run from any directory; output is adjacent to this script.
"""
from decimal import Decimal as D, localcontext
from pathlib import Path
import random
import struct

def pi():
    a, b, t, p = D(1), D(1)/D(2).sqrt(), D(1)/4, D(1)
    for _ in range(10):
        c=(a+b)/2
        b=(a*b).sqrt()
        t-=p*(a-c)**2
        a=c
        p*=2
    return (a+b)**2/(4*t)

def trig(x):
    period=2*pi()
    x-=((x/period).to_integral_value())*period
    sine=term_s=x
    cosine=term_c=D(1)
    for k in range(1,300):
        term_s*=-x*x/D((2*k)*(2*k+1))
        term_c*=-x*x/D((2*k-1)*(2*k))
        sine+=term_s
        cosine+=term_c
        if abs(term_s)+abs(term_c)<D('1e-160'):break
    return sine,cosine

def add(a,b):return(a[0]+b[0],a[1]+b[1])
def mul(a,b):return(a[0]*b[0]-a[1]*b[1],a[0]*b[1]+a[1]*b[0])
def matrix_mul(a,b):return[[add(mul(a[i][0],b[0][j]),mul(a[i][1],b[1][j])) for j in range(2)]for i in range(2)]

def residual(gates,axis,angle,precision):
    with localcontext() as c:
        c.prec=precision
        zero,one,negative=(D(0),D(0)),(D(1),D(0)),(D(-1),D(0))
        root=D(1)/D(2).sqrt()
        table={
            'H':[[(root,D(0)),(root,D(0))],[(root,D(0)),(-root,D(0))]],
            'X':[[zero,one],[one,zero]],'Y':[[zero,(D(0),D(-1))],[(D(0),D(1)),zero]],
            'Z':[[one,zero],[zero,negative]],'S':[[one,zero],[zero,(D(0),D(1))]],
            's':[[one,zero],[zero,(D(0),D(-1))]],'T':[[one,zero],[zero,(root,root)]],
            't':[[one,zero],[zero,(root,-root)]],'W':[[(root,root),zero],[zero,(root,root)]]}
        candidate=[[one,zero],[zero,one]]
        for gate in gates:candidate=matrix_mul(table[gate],candidate)
        theta=D.from_float(struct.unpack('>d',struct.pack('>Q',int(angle[1:])))[0]) if angle.startswith('D') else D(angle[1:].split('/')[0])/D(angle.split('/')[1])*pi()
        s,co=trig(theta/2)
        targets={
            'Z':[[(co,-s),zero],[zero,(co,s)]],
            'X':[[(co,D(0)),(D(0),-s)],[(D(0),-s),(co,D(0))]],
            'Y':[[(co,D(0)),(-s,D(0))],[(s,D(0)),(co,D(0))]]}
        target=targets[axis]
        return sum((candidate[i][j][k]-target[i][j][k])**2 for i in range(2)for j in range(2)for k in range(2))

def bits(value):return struct.unpack('>Q',struct.pack('>d',float(value)))[0]
rng=random.Random(0x3141512)
lines=['# gates axis angle epsilon_pass_bits epsilon_fail_bits residual_squared_lower residual_squared_upper; bounds denominator 10^50']
for case in range(24):
    gates=''.join(rng.choice('HXYZSsTtW')for _ in range(rng.randrange(1,25)))
    axis=rng.choice('XYZ')
    angle=f'D{bits(rng.uniform(-25,25))}' if case%2 else f'P{rng.randrange(-15,16)}/{rng.randrange(1,10)}'
    first=residual(gates,axis,angle,100)
    second=residual(gates,axis,angle,140)
    assert abs(first-second)<D('1e-90')
    with localcontext() as c:
        c.prec=140
        center=int(second*10**50)
        norm=second.sqrt()
        ep,bad=bits(norm+D('1e-9')),bits(norm-D('1e-9'))
    lines.append(f'{gates} {axis} {angle} {ep} {bad} {max(center-2,0)} {center+3}')
Path(__file__).with_name('rotation_oracle.txt').write_text('\n'.join(lines)+'\n')
print(f'{len(lines)-1} independent full-matrix residual goldens generated')
