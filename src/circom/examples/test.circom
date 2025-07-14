pragma circom 2.1.5;

include "node_modules/circomlib/circuits/comparators.circom";

// check s[i] < m, m must be power of 2
template RangeCheck(n, m) {
    signal input s[n];
    component bs[n];

    for (var i = 0; i < n; i++) {
        bs[i] = Num2Bits(nbits(m)-1);
        bs[i].in <== s[i];
    }
}

function Getn(pk_0, pk_1, pk_2, pk_3, pk_4, pk_5, pk_6, pk_7, pk_8, pk_9) {
   var n = 0;
   n += 1;n += 1;n += 1;n += 1;n += 1;n += 1;n += 1;n += 1;n += 1;n += 1;
   return n;
}

// c * q = s1 + s2 pk - h
template Falcon_correctness(pk_0, pk_1, pk_2, pk_3, pk_4, pk_5, pk_6, pk_7, pk_8, pk_9) {
    var n = Getn(pk_0, pk_1, pk_2, pk_3, pk_4, pk_5, pk_6, pk_7, pk_8, pk_9);
    signal input s1[n];
    signal input s2[n];
    signal input h[n];
    signal input c[n];

    var rhs[n];
    var lhs[n];
    var q = 12289;

    for (var i = 0; i < n; i++) {
      rhs[i] = s1[i] - h[i];
      rhs[i] += s2[(n+i-0)%n] * pk_0;
      rhs[i] += s2[(n+i-1)%n] * pk_1;
      rhs[i] += s2[(n+i-2)%n] * pk_2;
      rhs[i] += s2[(n+i-3)%n] * pk_3;
      rhs[i] += s2[(n+i-4)%n] * pk_4;
      rhs[i] += s2[(n+i-5)%n] * pk_5;
      rhs[i] += s2[(n+i-6)%n] * pk_6;
      rhs[i] += s2[(n+i-7)%n] * pk_7;
      rhs[i] += s2[(n+i-8)%n] * pk_8;
      rhs[i] += s2[(n+i-9)%n] * pk_9;

      lhs[i] = c[i] * q;
      lhs[i] === rhs[i];
    }
}

template Falcon(pk_0, pk_1, pk_2, pk_3, pk_4, pk_5, pk_6, pk_7, pk_8, pk_9) {
    var n = Getn(pk_0, pk_1, pk_2, pk_3, pk_4, pk_5, pk_6, pk_7, pk_8, pk_9);
    signal input s1[n];
    signal input s2[n];

    // range check for s1, s2
    component _s1 = RangeCheck(n, 4096);
    for (var i = 0; i < n; i++) {
        _s1.s[i] <== s1[i];
    }

    component _s2 = RangeCheck(n, 4096);
    for (var i = 0; i < n; i++) {
        _s2.s[i] <== s2[i];
    }

    component _c = Falcon_correctness(pk_0, pk_1, pk_2, pk_3, pk_4, pk_5, pk_6, pk_7, pk_8, pk_9);
}

component main = Falcon_correctness(225, 189, 31, 79, 183, 87, 45, 1, 170, 104);
