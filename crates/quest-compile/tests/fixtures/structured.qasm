OPENQASM 3.1;
include "stdgates.inc";
gate bell a,b { h a; cx a,b; }
qubit[2] q;
bell q[0],q[1];
