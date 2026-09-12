> **Source:** Hiroshi Nagamochi, *Packing Unit Squares in a Rectangle*, Electronic Journal of Combinatorics 12 (2005) #R37. https://doi.org/10.37236/1934
>
> **License:** Copyright held by the author(s). Published by the Electronic Journal of Combinatorics under a non-exclusive publication agreement; no open license was attached (pre-2018 EJC paper).
>
> Math transcription checked against [`../downloads/nagamochi-2005.pdf`](../downloads/nagamochi-2005.pdf). Missing formulas and lemma statements were restored from the original. Figure captions link to source pages. Apparent errors printed in the source are retained and identified in the transcription notes below; consult the PDF for authoritative wording.

# Packing Unit Squares in a Rectangle

Hiroshi Nagamochi

Department of Applied Mathematics and Physics, Kyoto University

Sakyo, Kyoto-city, Kyoto 606-8501, Japan

`nag@amp.i.kyoto-u.ac.jp`

Submitted: Sep 29, 2004; Accepted: Jul 8, 2005; Published: Jul 30, 2005.

Mathematics Subject Classifications: 05B40, 52C15

## Abstract

For a positive integer $N$, let $s(N)$ be the side length of the minimum square into which $N$ unit squares can be packed. This paper shows that, for given real numbers $a,b\ge 2$, no more than $ab-(a+1-\lceil a\rceil)-(b+1-\lceil b\rceil)$ unit squares can be packed in any $a'\times b'$ rectangle $R$ with $a'<a$ and $b'<b$. From this, we can deduce that, for any integer $N\ge 4$,

```math
s(N)\ge \min\left\{\lceil\sqrt{N}\rceil,\sqrt{N-2\lfloor\sqrt{N}\rfloor+1}+1\right\}.
```

In particular, for any integer $n\ge 2$, $s(n^2)=s(n^2-1)=s(n^2-2)=n$ holds.

## 1 Introduction

Packing geometric objects such as circles and squares into another object is one of the fundamental problems in combinatorial geometry [1, 2, 4]. For a positive integer $N$, let $s(N)$ be the side length of the minimum square that can contain $N$ unit squares in the plane whose interiors do not overlap. The problem of packing unit squares into a square was initiated by Erdős and Graham [2]. They prove that, for a large number $s$, unit squares can be packed into an $s\times s$ square so that the wasted area is $O(s^{7/11})$. This is surprisingly small compared with the wasted area in the ‘trivial’ packing of $N=n^2-n$ unit squares in an $n\times n$ square, where $n$ is an integer more than 1.

Determining or estimating $s(N)$ is posed as one of the unsolved geometric problems listed by Croft et al. [1]. We easily observe that for any positive integer $N$, $\sqrt{N}\le s(N)\le\lceil\sqrt{N}\rceil$, and that for any square number $N=n^2$, $s(N)=n$. It was conjectured that $s(n^2-n)=n$ holds for integers $n\ge 2$ (whenever $n$ is small). For $n\ge 17$, $s(n^2-n)<n$ is demonstrated by an explicit construction (see [3]). Friedman [3] conjectures that, once $s(n^2-k)=n$ holds for some integers $n$ and $k$, $s((n+1)^2-k)=n+1$ holds. Determining $s(N)$ for non-square numbers $N$ seems rather difficult. Currently such $s(N)$ has been determined only for some limited numbers $N<100$ (see [3, 5]). These nontrivial values for $s(N)$ are based on lower bounds which are established in a particular way for each $N$.

In this paper, we introduce a lower bound on $s(N)$ that is systematically constructible for any integer $N\ge 4$. For two positive real numbers $a$ and $b$, let $\nu(a,b)$ denote the maximum number of unit squares that can be packed into the inside of an $a'\times b'$ rectangle $R$ with $a'<a$ and $b'<b$. A trivial upper bound on $\nu(a,b)$ is $\nu(a,b)<ab$. In this paper, we prove the following result.

**Theorem 1.** For real numbers $a,b\ge 2$,

```math
\nu(a,b)<ab-(a+1-\lceil a\rceil)-(b+1-\lceil b\rceil).
```

In particular, for two integers $a\ge b\ge 2$, we see that an $a\times b$ rectangle is the smallest rectangle with aspect ratio $a/b$ into which $ab-2$ unit squares can be packed. Theorem 1 also provides a new lower bound on $s(N)$, determining $s(N)$ for infinitely many new numbers $N$.

**Theorem 2.**

- (i) For any positive integer $N$ such that $N\in\{n^2,n^2-1,n^2-2\}$ for some integer $n\ge 1$, $s(N)=n$ holds.
- (ii) For any integer $N\ge 4$ such that $N\notin\{n^2,n^2-1,n^2-2\mid\text{integers }n\ge 1\}$,

```math
s(N)\ge\sqrt{N-2\lfloor\sqrt{N}\rfloor+1}+1>\sqrt{N}.
```

Note that our new lower bound in Theorem 2(ii) is strictly stronger than the trivial lower bound $\sqrt{N}$. This paper is organized as follows. After deriving Theorem 2 from Theorem 1 in section 2, we define an unavoidable set $U$ in section 3, showing that proving the unavoidability of $U$ implies Theorem 1. In section 5, we present a proof for the unavoidability of $U$ after preparing a series of technical lemmas in section 4. We make concluding remarks in section 5.[^section]

## 2 Proof of Theorem 2

This section shows that Theorem 2 follows from Theorem 1. Any square number $N=n^2$ satisfies

```math
s(N)=n=\sqrt{N}=\lceil\sqrt{N}\rceil=\sqrt{N-2\lfloor\sqrt{N}\rfloor+1}+1
```

and inequality $\sqrt{N+2}\ge\lceil\sqrt{N}\rceil$. Now assume that $\sqrt{N}$ is not an integer, for which $\lceil\sqrt{N}\rceil=\lfloor\sqrt{N}\rfloor+1$ holds. Then we have

```math
\begin{aligned}
\sqrt{N-2\lfloor\sqrt{N}\rfloor+1}+1
&=\sqrt{N-(\lceil\sqrt{N}\rceil)^2+(\lfloor\sqrt{N}\rfloor+1)^2-2\lfloor\sqrt{N}\rfloor+1}+1\\
&=\sqrt{N+2-(\lceil\sqrt{N}\rceil)^2+(\lfloor\sqrt{N}\rfloor)^2}+1.
\end{aligned}
```

Hence $\sqrt{N-2\lfloor\sqrt{N}\rfloor+1}+1\ge\lfloor\sqrt{N}\rfloor+1$ if and only if $\sqrt{N+2}\ge\lceil\sqrt{N}\rceil$. A positive integer $N$ satisfies $\sqrt{N+2}\ge\lceil\sqrt{N}\rceil$ if and only if there is an integer $n$ such that $\sqrt{N+2}\ge n\ge\sqrt{N}$, i.e., $n^2\ge N\ge n^2-2$. It is known that $s(1)=1$ and $s(2)=s(3)=s(4)=2$ [4]. Let $N\ge 4$.

We first consider the case where $\sqrt{N+2}\ge\lceil\sqrt{N}\rceil$. Then by Theorem 1 with $a=b=\lceil\sqrt{N}\rceil\ge 2$, we have $\nu(\lceil\sqrt{N}\rceil,\lceil\sqrt{N}\rceil)<(\lceil\sqrt{N}\rceil)^2-2\le N$. This says that $N$ unit squares cannot be packed in any square with side length less than $\lceil\sqrt{N}\rceil$. Thus, $s(N)\ge\lceil\sqrt{N}\rceil$. So for any integer $N\in\{n^2,n^2-1,n^2-2\}$, where $n\ge 1$ is an integer, we have $s(N)\ge n=\lceil\sqrt{N}\rceil\ge s(N)$. This proves (i).

We next consider the case where $\sqrt{N+2}<\lceil\sqrt{N}\rceil$. Let $k=\lfloor\sqrt{N}\rfloor\ge 2$ and $\alpha=\sqrt{N-2\lfloor\sqrt{N}\rfloor+1}-\lfloor\sqrt{N}\rfloor+1$. Note that $\alpha$ is a solution to $(\alpha+k-1)^2=N-2k+1$. Note that $\alpha<1$ since $\sqrt{N+2}<\lceil\sqrt{N}\rceil$. Hence by Theorem 1 with $a=b=k+\alpha\ge 2$, we have

```math
\begin{aligned}
\nu(k+\alpha,k+\alpha)
&<(k+\alpha)^2-2(\alpha+1-\lceil\alpha\rceil)\\
&=(k+\alpha)^2-2\alpha\\
&=(k+\alpha-1)^2+2k-1=N.
\end{aligned}
```

Therefore, $N$ unit squares cannot be packed in any square with side length less than $k+\alpha=\sqrt{N-2\lfloor\sqrt{N}\rfloor+1}+1$. Thus, $s(N)\ge\sqrt{N-2\lfloor\sqrt{N}\rfloor+1}+1$. Furthermore, we see that

```math
\sqrt{N-2\lfloor\sqrt{N}\rfloor+1}+1>\sqrt{N-2\sqrt{N}+1}+1=\sqrt{N}.
```

This proves (ii).

## 3 Unavoidable Sets

The conventional method for deriving a lower bound on $s(N)$ [3] is as follows. Suppose that we wish to show $s(N)\ge a$. Let $R$ be a square with side length *less* than $a$, and $U$ be a set of some points inside $R$, where $U$ is called *unavoidable* if any unit square placed inside $R$ must contain at least one point from $U$. If we successfully obtain an avoidable set $U$ with $\lvert U\rvert<N$, then we can conclude that $\lvert U\rvert+1$ unit squares cannot be packed inside $R$, i.e., $s(N)\ge s(\lvert U\rvert+1)\ge a$.[^avoidable] For example, let $N=2$. Take a square $R$ with side length less than $a=2$. Then we easily see that $U$ consisting of the center of $R$ is unavoidable, and thereby we need a square $R$ with side length at least $a=2$ to pack two unit squares, i.e., $s(2)\ge 2$. An unavoidable set $U$ with $\lvert U\rvert<N$ over a smaller square $R$ provides a better lower bound on $s(N)$. Only for few integers $N<100$, have such unavoidable sets been constructed to obtain nontrivial lower bounds on $s(N)$. However, these constructions are not systematic in terms of $N$, providing no general lower bound on $s(N)$ for large $N$.

In this paper, we use not only points but also other geometric objects such as line segments and rectangles to define our unavoidable set $U$. Recall that the trivial lower bound $s(N)\ge\sqrt{N}$ follows from the fact that each unit square consumes at least area 1 from the entire square $R$, where $R$ can be regarded as an unavoidable set from which unit square takes score 1.

In the $xy$-plane, a line segment $L$ connecting two points $p_1=(x_1,y_1)$ and $p_2=(x_2,y_2)$ is denoted by $L=[p_1,p_2]$ or $L=[(x_1,y_1),(x_2,y_2)]$. A rectangle $R'$ with edges parallel with $x$-, $y$-axes may be written as $[x_1,x_2]\times[y_1,y_2]$ if the four corners of $R'$ are given by $(x_1,y_1)$, $(x_1,y_2)$, $(x_2,y_1)$ and $(x_2,y_2)$ for real numbers $x_1\le x_2$ and $y_1\le y_2$.

To prove $\nu(a,b)<ab-(a+1-\lceil a\rceil)-(b+1-\lceil b\rceil)$ for given real numbers $a,b\ge 2$, we consider a rectangle $R=[0,a]\times[0,b]$ in the $xy$-plane. Let $U$ consist of a rectangle $R^*$, four lines $L_i$ ($i=1,2,3,4$), a set $Q$ of eight points, and a set $P$ of $2\lceil a\rceil+2\lceil b\rceil-12$ points, such that

```math
\begin{aligned}
R^*&=[1,a-1]\times[1,b-1],\\
L_1&=[(0.9,1),(a-0.9,1)],& L_2&=[(0.9,b-1),(a-0.9,b-1)],\\
L_3&=[(1,0.9),(1,b-0.9)],& L_4&=[(a-1,0.9),(a-1,b-0.9)],\\
Q&=\{(0.9,1),(a-0.9,1),(0.9,b-1),(a-0.9,b-1),\\
&\qquad (1,0.9),(1,b-0.9),(a-1,0.9),(a-1,b-0.9)\},\\
P&=\{(i,0.9),(i,a-0.9)\mid i=2,3,\ldots,\lceil a\rceil-2\}\\
&\quad\cup\{(0.9,j),(b-0.9,j)\mid j=2,3,\ldots,\lceil b\rceil-2\}.
\end{aligned}
```

See Fig. 1.[^coordinates]

**Figure 1:** An unavoidable set $U$ for a rectangle $R=[0,a]\times[0,b]$. [View the diagram (PDF, p. 4).](../downloads/nagamochi-2005.pdf#page=4)

Let $\lambda>1$. We say that $R$ and $U$ are *shrunken* toward the origin $(0,0)$ by factor $\lambda^{-1}$ if we map each point $(x,y)$ in $R$ and $U$ to a new point $(\lambda^{-1}x,\lambda^{-1}y)$. Let $\lambda^{-1}R$ and $\lambda^{-1}U$ respectively denote such $R$ and $U$ shrunken by factor $\lambda^{-1}$.

For a given unit square $S$ inside $\lambda^{-1}R$ and an object $K\in\{Q,P,L_1,L_2,L_3,L_4,R^*\}$, we define *score* $\sigma(S;K)$ of $S$ by $K$ as follows.

- $\sigma(S;R^*)$ = (the area of the intersection of $S$ and $R^*$) $\times\lambda^2$,
- $\sigma(S;L_i)$ = (the sum of length of the intersection of $S$ and line segment $L_i$) $\times 0.5\times\lambda$,
- $\sigma(S;Q)$ = (the number of points in $Q$ contained in $S$) $\times 0.45$, and
- $\sigma(S;P)$ = (the number of points in $P$ contained in $S$) $\times 0.5$.

Define

```math
\sigma(S)=\sigma(S;R^*)+\sigma(S;L_1)+\sigma(S;L_2)+\sigma(S;L_3)+\sigma(S;L_4)+\sigma(S;Q)+\sigma(S;P).
```

Note that the total score from $L_i$ ($i=1,2,3,4$) and $Q$ is $2(a-1.8)\times 0.5+2(b-1.8)\times 0.5+8\times 0.45=a+b$. Then the total score from $U$ is $(a-2)(b-2)+a+b+\lceil a\rceil-3+\lceil b\rceil-3=ab-(a+1-\lceil a\rceil)-(b+1-\lceil b\rceil)$. In what follows, we prove that $U$ is an unavoidable set in the following sense.

**Lemma 1.** Any unit square $S$ inside $\lambda^{-1}R$ satisfies $\sigma(S)>1$.

We show that Theorem 1 follows from Lemma 1. Assume that $N'$ unit squares are packed inside $\lambda^{-1}R$. Each of the $N'$ unit squares has $\sigma(S)>1$ by Lemma 1 and the total score of $U$ is $ab-(a+1-\lceil a\rceil)-(b+1-\lceil b\rceil)$. Then we have $N'<ab-(a+1-\lceil a\rceil)-(b+1-\lceil b\rceil)$ for any factor $\lambda^{-1}<1$, i.e., $\nu(a,b)<ab-(a+1-\lceil a\rceil)-(b+1-\lceil b\rceil)$, as required.

A square $S$ with side length $\lambda$ is called a $\lambda\times\lambda$ square. For a notational convenience to prove Lemma 1, we consider packing $\lambda\times\lambda$ squares with $\lambda>1$ into the original rectangle $R=[0,a]\times[0,b]$, instead of considering $\lambda^{-1}R$ and $\lambda^{-1}U$. In this case, each $L_i$ contributes to $\sigma(S)$ by 0.5 per length and $R^*$ by 1 per area while each point in $Q$ (resp., $P$) contributes to $\sigma(S)$ by 0.45 (resp., 0.5). It suffices to show that any $\lambda\times\lambda$ square $S$ with $\lambda\in(1,1.01]$ has $\sigma(S)>1$ over the original $R$ and $U$.

## 4 Technical Lemmas

In this section, we prepare some technical lemmas in order to establish a proof of Lemma 1 in the next section. Let $\lambda\in[1,1.01]$ for a technical reason to prove the lemmas in this section.

**Lemma 2.** Let $S$ be a $\lambda\times\lambda$ square with $\lambda\in[1,1.01]$. For a line $L$ with distance $h\in[0,(\sqrt{2}-1)/2)$ from the center of $S$, let $c$ be the length of the intersection of $S$ and $L$ (see Fig. 2(a)). Then $c\ge\lambda$ or $c>1$.

**Proof:** Let $L$ intersect edges $e_1$ and $e_2$ of $S$. If $e_1$ and $e_2$ are not adjacent, then $c\ge\lambda$. We consider the case where $e_1$ and $e_2$ are adjacent. We can assume that $\lambda=1$ to estimate the minimum $c$. Let $\theta$ denote the angle made by $L$ and $e_2$, where $0<\theta\le\pi/4$ is assumed without loss of generality. Let $t=\tan(\theta/2)$, where $0<t=\tan(\theta/2)\le\sqrt{2}-1$ for $\theta\in(0,\pi/4]$. Then we have

```math
c=-h\frac{(1+t^2)^2}{2t(1-t^2)}+\frac{(1+t^2)(1+2t-t^2)}{4t(1-t^2)},
```

which is a decreasing function of $h$ for a fixed $t$. Hence it suffices to show that $f(h,t)=-2h(1+t^2)^2+(1+t^2)(1+2t-t^2)-4t(1-t^2)$ is nonnegative for $h=(\sqrt{2}-1)/2$. We have

```math
f\left(\frac{\sqrt{2}-1}{2},t\right)=(t+1-\sqrt{2})^2\left(-\sqrt{2}t^2+(2+2\sqrt{2})t+2+\sqrt{2}\right).
```

By the concavity of $g(t)=-\sqrt{2}t^2+(2+2\sqrt{2})t+2+\sqrt{2}$, $g(0)>0$ and $g(\sqrt{2}-1)>0$ mean $g(t)>0$ ($0<t\le\sqrt{2}-1$). Hence $f((\sqrt{2}-1)/2,t)\ge 0$ and $c>1$.

**Lemma 3.** Let $S$ be a $\lambda\times\lambda$ square with $\lambda\in[1,1.01]$ such that one corner of $S$ touches the $x$-axis and $S$ is entirely above the $x$-axis. For a line $L:y=h$ with $h\in(0.5,\sqrt{2}-0.5)$, let $c$ be the length of the intersection of $S$ and $L$ (see Fig. 2(b)). Then $c\ge\lambda$ or $c>1$.

**Figure 2:** (a) Illustration for Lemma 2; (b) Illustration for Lemma 3. [View the diagrams (PDF, p. 6).](../downloads/nagamochi-2005.pdf#page=6)

**Proof:** Let $L$ intersect edges $e_1$ and $e_2$ of $S$. We consider the case where $e_1$ and $e_2$ are adjacent (otherwise $c\ge\lambda$). By $h>0.5$, both $e_1$ and $e_2$ are not touching the $x$-axis. We can assume that $\lambda=1$ to estimate the minimum $c$. Let $\theta$ be angle made by $L$ and $e_2$, where $0<\theta\le\pi/4$ is assumed without loss of generality. Let $t=\tan(\theta/2)$, where $0<t=\tan(\theta/2)\le\sqrt{2}-1)$ for $\theta\in(0,\pi/4]$.[^parenthesis] Then we have

```math
c=-(h-1)\frac{(1+t^2)^2}{2t(1-t^2)}+\frac{2t(1-t+t^2-t^3)}{2t(1-t^2)},
```

which is a decreasing function of $h$ for a fixed $t$. To prove the lemma, it suffices to show that $f(h,t)=-(h-1)(1+t^2)^2+2t-2t^2+2t^3-2t^4-2t+2t^3\ge 0$ for $h=\sqrt{2}-0.5$. We see that

```math
f(\sqrt{2}-0.5,t)=(t+1-\sqrt{2})^2\left(-(1+2\sqrt{2})t^2+(2\sqrt{2}+2)t+1\right).
```

By the concavity of $g(t)=-(1+2\sqrt{2})t^2+(2\sqrt{2}+2)t+1$, $g(0)>0$ and $g(\sqrt{2}-1)>0$ mean $g(t)>0$ ($0<t<\sqrt{2}-1$). Therefore $f(\sqrt{2}-0.5,t)\ge 0$ and $c>1$.[^factorization]

**Lemma 4.** Let $S$ be a $\lambda\times\lambda$ square with $\lambda\in[1,1.01]$, and $e_1$ and $e_2$ be two adjacent edges of $S$ that meet at a corner $v$ of $S$. For a point $p_1$ on $e_1$ and a point $p_2$ on $e_2$ with $p_1\ne v\ne p_2$, let $c$ be the length of the line segment $L=[p_1,p_2]$, and $d$ be the area of the triangle enclosed by $L$ and line segments $[p_1,v]$ and $[v,p_2]$ (see Fig. 3). Then $0.5c>d$.

**Proof:** Let $h$ and $\ell$ be the lengths of the line segments $[p_1,v]$ and $[v,p_2]$, respectively. Then $c=\sqrt{h^2+\ell^2}$ and $d=h\ell/2$. To prove $c/2>d$, it suffices to show that $h^2+\ell^2-h^2\ell^2>0$. Since $h,\ell\in(0,\lambda]$, we have $h^2+\ell^2-h^2\ell^2=(h-\ell)^2+h\ell(2-h\ell)\ge h\ell(2-\lambda^2)>0$.

**Figure 3:** Illustration for Lemma 4. [View the diagram (PDF, p. 7).](../downloads/nagamochi-2005.pdf#page=7)

**Lemma 5.** Let $S$ be a $\lambda\times\lambda$ square with $\lambda\in[1,1.01]$ such that one corner of $S$ touches the $x$-axis and $S$ is entirely above the $x$-axis, $c>0$ be the length of the intersection of $S$ and line $L:y=1$, and $d$ be the area of the triangle enclosed by $S$ and $L$ (see Fig. 4(a)). Then $d+0.5c>0.5$.

**Proof:** Let $\theta\in(0,\pi/4]$ be the angle made by an edge of $S$ and the $x$-axis, and $t=\tan(\theta/2)$. We obtain $d=c^2\times t(1-t^2)/(1+t^2)^2$. We denote $c$ and $d$ for $\lambda=1$ by $\bar c$ and $\bar d$. Then we have $1-\bar c=\bar d=(t-t^2)/(1+t)$, for which $\bar d+0.5\bar c=1-\bar c+0.5\bar c=0.5+0.5(1-\bar c)>0.5$. Now consider the case of $\lambda>1$. Since $\lambda-1$ is small, we can write $c=\bar c+x$ and

```math
d=(\bar c+x)^2\times\frac{t(1-t^2)}{(1+t^2)^2}=\bar d+(2\bar c+x^2)\times\frac{t(1-t^2)}{(1+t^2)^2}
```

for some number $x>0$. Then

```math
d+0.5c=\bar d+0.5\bar c+0.5x+(2\bar c+x^2)\times\frac{t(1-t^2)}{(1+t^2)^2}\ge\bar d+0.5\bar c>0.5.
```

[^expansion]

**Figure 4:** (a) Illustration for Lemma 5; (b) Illustration for Lemma 6. [View the diagrams (PDF, p. 7).](../downloads/nagamochi-2005.pdf#page=7)

**Lemma 6.** Let $S$ be a $\lambda\times\lambda$ square with $\lambda\in[1,1.01]$ such that one corner of $S$ touches the $x$-axis and $S$ is entirely above the $x$-axis. Assume that two adjacent edges $e_1$ and $e_2$ of $S$ intersect line $L:y=1$, point $(1,1)$ is not in $S$, point $(2,0.9)$ is on an edge $e_2$ of $S$. Let $c$ be the length of the intersection of $S$ and $L$, $d$ be the area of the triangle enclosed by $S$ and $L$, and $p'=(1,1-c')$ be the crossing point of $e_1$ and line $x=1$ (see Fig. 4(b)). Then $d+0.5c+0.5-0.5c'>1$ holds.

**Proof:** For values $d$, $c$, $-c'$ for a $\lambda\times\lambda$ square $S$ with $\lambda>1$, we can get smaller $d$, $c$, $-c'$ choosing a $\lambda'\times\lambda'$ square $S$ with $1\le\lambda'<\lambda$. Then we only consider the case of $\lambda=1$. Let $\theta\in(0,\pi/2]$ be the angle made by $e_1$ and $L:y=1$. By calculation, we have $d=t(1-t)/(1+t)$, $c=(t+t^2)/(1+t)$, and $c'=2t(t(1-t)^2-0.2t)/(1-t^2)^2$. To have $c'>0$ (i.e., to keep $(1,1)$ outside $S$), $t(1-t)^2-0.2t>0$ (i.e., $t<1-\sqrt{0.2}$) must hold. Note that $c=1-d$ holds.[^lemma6] To prove $d+0.5c+0.5-0.5c'>1$, it suffices to show that $d>c'$, i.e.,

```math
\frac{t(1-t)}{1+t}>\frac{2t(t(1-t)^2-0.2t)}{(1-t^2)^2},\qquad(0<t<1-\sqrt{0.2}).
```

For this, we show $f(t)=(1-t)^2(1-t^2)-2(t(1-t)^2-0.2t)\ge 0$. We have $f(t)=(1-t)^2(2-(1+t)^2)+0.4t$, which is positive for $0<t\le\sqrt{2}-1$. On the other hand, for $0.41<\sqrt{2}-1<t<1-\sqrt{0.2}<0.56$, we have $(1-0.41)^2(2-(1+0.56)^2)+0.4\cdot0.41>0$. This completes the proof of the lemma.

## 5 Proof of Lemma 1

Throughout this section, $S$ denotes a $\lambda\times\lambda$ square with $\lambda\in(1,1.01]$ that is entirely contained in a given $a\times b$ rectangle $R=[0,a]\times[0,b]$. We prove that $\sigma(S)>1$, from which Lemma 1 follows. We distinguish the following seven cases:

- **Case-1:** $S$ is contained completely inside $R^*=[1,a-1]\times[1,b-1]$.
- **Case-2:** $S$ is not completely contained inside $R^*$, the center of $S$ is inside $R^*$, $S$ does not contain any point in $Q$ as its interior point, and there is no line segment $L_i\in U$ that intersects two nonadjacent edges of $S$.
- **Case-3:** The center of $S$ is inside $R^*$, $S$ does not contain any point in $Q$ as its interior point, and there is a line segment $L_i$ that intersects two nonadjacent edges of $S$.
- **Case-4:** The center of $S$ is inside $R^*$, and $S$ contains a point in $Q$ as its interior point.
- **Case-5:** The center of $S$ belongs to the rectangle $[0,1]\times[0,1]$.
- **Case-6:** The center of $S$ belongs to the rectangle $[1,a-1]\times[0,1]$, and line $y=1$ intersects two adjacent edges of $S$.
- **Case-7:** The center of $S$ belongs to the rectangle $[1,a-1]\times[0,1]$, and line $y=1$ intersects two nonadjacent edges of $S$.

The case where the center of $S$ belongs to one of the rectangles $[a-1,a]\times[0,1]$, $[0,1]\times[b-1,b]$ and $[a-1,a]\times[b-1,b]$ can be treated analogously with Case-5. Also the case where the center of $S$ belongs to one of the rectangles $[1,a-1]\times[b-1,b]$, $[0,1]\times[1,b-1]$ and $[a-1,a]\times[1,b-1]$ can be treated in a similar way of Cases-6 and 7.

In Case-1, we easily see that $\sigma(S)\ge\sigma(S;R^*)=\lambda^2>1$ holds. The rest of the cases will be discussed in the subsequent subsections.

### 5.1 Case-2

In this case, $S$ is not completely contained inside $R^*$, the center of $S$ is inside $R^*$, $S$ does not contain any point in $Q$ as its interior point, and there is no line segment $L_i\in U$ that intersects two nonadjacent edges of $S$. Then there is a line segment $L_i\in U$ that intersects two adjacent edges of $S$, cutting out from $S$ a triangle $T_i$ that is not covered by $R^*$ (see Fig. 5). For each of all those line segments $L_i$, let $d_i$ be the area of the triangle $T_i$, and $c_i$ be the length of the intersection of $L_i$ and $S$ (some of these triangles may be overlapping, as illustrated by $S_3$ in Fig. 5). By Lemma 4, we have $0.5c_i-d_i>0$ for all such $L_i$. This implies that $\sigma(S;L_i)=0.5c_i$ compensates the loss $d_i$ in $\sigma(S;R^*)$. Thus $\sigma(S)$ is not less than that of a $\lambda\times\lambda$ square $S$ which is completely contained in $R^*$. Therefore, $\sigma(S)>1$.

**Figure 5:** Illustration for $\lambda\times\lambda$ squares in Case-2. [View the diagram (PDF, p. 9).](../downloads/nagamochi-2005.pdf#page=9)

### 5.2 Case-3

In this subsection, we consider the case where the center of $S$ is inside $R^*$, $S$ does not contain any point in $Q$ as its interior point, and there is a line segment $L_i$ that intersects two nonadjacent edges of $S$. The length of the intersection of $L_i$ and $S$ is at least $\lambda>1$. Then if there are two such line segments $L_i$ and $L_{i'}$, then $\sigma(S)\ge\sigma(S;L_i)+\sigma(S;L_{i'})\ge\lambda\times 0.5\times 2>1$. Assume that there is exactly one such line segment $L_i$, which cuts out from $S$ an quadrangle uncovered by $R^*$. From the above observation using Lemma 4, we can assume that there is no other line segment $L_j\in U$ that cuts out from $S$ an uncovered triangle $T_j$. Since the center is in $R^*$ and $\sigma(S;R^*)\ge 0.5\lambda^2$, we have $\sigma(S)\ge\sigma(S;R^*)+\sigma(S;L_i)\ge 0.5\lambda^2+0.5\lambda>1$.

### 5.3 Case-4

In this case, the center of $S$ is inside $R^*$, $S$ contains a point in $Q$ as its interior point. We show that this case can be reduced to Case-2. Assume that $S$ contains point $(1,0.9)$ (the case where $S$ contains other point in $Q$ can be treated analogously). To estimate the minimum $\sigma(S)$, we temporarily replace the point $(1,0.9)$ with line segment $L'=[(1,0.9),(1,0)]$, setting the score of $L'$ per length to be 0.5 (note that the total score of $L'$ is 0.45, the same as that of point $(1,0.9)$). If $S$ contains other points in $Q$, we replace each of them in a similar manner. With this modification, the score of $S$ never increases and the argument in Case-2 can be applied, indicating $\sigma(S)>1$.

### 5.4 Case-5

We start with the following lemma to handle Cases-5, 6 and 7.

**Lemma 7.** Let $S$ be a $\lambda\times\lambda$ square with $\lambda\in(1,1.01]$ that is entirely contained in $R$. Assume that the center of $S$ belongs to the rectangle $[0,a]\times[0,1]$. Then

- (i) The length $c$ of the intersection of $S$ and line $L:y=0.9$ is more than 1.
- (ii) If the center of $S$ belongs to the square $[0,1]\times[0,1]$, then $S$ contains three points $(1,1)$ and $(1,0.9),(0.9,1)\in Q$ as its interior points.
- (iii) $S$ contains at least one point in $Q\cup P$.
- (iv) $\sigma(S;R^*)>0$.

**Proof:** (i) If $L$ intersects two nonadjacent edges of $S$, then $c\ge\lambda>1$. Assume that $L$ intersects two adjacent edges of $S$. If the center is below $L$ then we only have to consider the case where one corner of $S$ touches the $x$-axis, and in this case $c>1$ follows from Lemma 3 with $h=0.9$. In the other case (i.e., the center of $S$ is situated between $L$ and line $y=1$), $c>1$ holds by Lemma 2 with $h=0.1$.

(ii) It is known that any unit square inside the first quadrant whose center is in $[0,1]\times[0,1]$ contains the point $(1,1)$ (for example, see [3]). Then $S$ contains $(1,1)$ as its interior point since $\lambda>1$. We show that $S$ contains $(1,0.9)$ (we can show that $S$ contains $(0.9,1)$ analogously). By (i), $S$ contains one of the points $(0,0.9)$ and $(1,0.9)$. Assume that $S$ contains $(0,0.9)$ but not $(1,0.9)$. This can occur only when one corner of $S$ attaches the $y$-axis at the point $(0,0.9)$. Let $e$ and $e'$ be the edges of $S$ that are not incident to the point $(0,0.9)$. Since any point on $e$ and $e'$ has distance at least $\lambda>1$ from the $(0,0.9)$, $S$ must contain $(1,0.9)$ as its interior point.

(iii) Immediate from (i) and (iii).[^reference]

(iv) We easily see that $\sigma(S;R^*)>0$ holds from $\lambda>1$ if the center of $S$ belongs to the rectangle $[1,a-1]\times[0,1]$; $\sigma(S;R^*)>0$ holds from (ii) otherwise.

In Case-5, the center of $S$ belongs to the rectangle $[0,1]\times[0,1]$. Then by Lemma 7(ii) $S$ contains $(1,0.9),(0.9,1)\in Q$ and line segments $[(1,0.9),(1,1)]$ and $[(0.9,1),(1,1)]$, and thereby $\sigma(S)\ge\sigma(S;R^*)+0.45\times 2+0.1\times 2\times 0.5>1$.

### 5.5 Case-6

In this case, the center of $S$ belongs to the rectangle $[1,a-1]\times[0,1]$, and line $y=1$ intersects two adjacent edges $e_1$ and $e_2$ of $S$. Let $L'$ be the line segment obtained as the intersection of line $y=1$ and $S$, $c$ be the length of $L'$, and $d$ be the area of the triangle $T$ enclosed by $L'$, $e_1$ and $e_2$.

We first assume that $S$ contains a point in $P$ and none of points $(0.9,1),(a-0.9,1)\in Q$. Then $L'$ is contained in $L_1$. To estimate the minimum of $d+0.5c$ in this case, we can assume that one corner of $S$ touches the $x$-axis. Assume that triangle $T$ is contained in $R^*$. Since $S$ contains $T$, $L'$ and a point in $P$, we have $\sigma(S)\ge d+0.5c+0.5>1$ by Lemma 5. Even if $T$ has an intersection with $L_2$, $\sigma(S)>1$ still holds by applying Lemma 4 to the triangle $T'$ enclosed by $L_2$ and $S$.

We next consider the case where $S$ contains a point in $P$ and at least one of points $(0.9,1),(a-0.9,1)\in Q$ (say $(0.9,1)$). Then $S$ contains line segment $[(0.9,1),(1,1)]$ and hence $\sigma(S)\ge\sigma(S;R^*)+0.5+0.45+0.05>1$.

We now consider the case where $S$ contains no point in $P$. Then, by Lemma 7(iii), $S$ contains at least one of points $(1,0.9),(a-1,0.9)\in Q$. Assume that $S$ contains $(1,0.9)$ (the case that $S$ contains $(a-1,0.9)$ can be treated analogously). If $S$ contains point $(0.9,1)$, then it contains line segments $[(0.9,1),(1,1)]$ and $[(1,0.9),(1,1)]$, and $\sigma(S)\ge\sigma(S;R^*)+(0.45+0.1\times 0.5)\times 2>1$ holds. Similarly if $S$ contains $(a-1,0.9)$, then we can show that $S$ contains line segments $[(a-1,0.9),(a-1,1)]$ and $[(a-0.9,1),(a-1,1)]$, implying $\sigma(S)\ge 1$. Then assume that $S$ contains none of points $(0.9,1)$ and $(a-1,0.9)$. If $S$ contains point $(\min\{2,a-1\},0.9)$ then $\sigma(S)\ge 0.45\times 2+d+0.5c>1$ by Lemma 5. Assume further that $S$ does not contain point $(\min\{2,a-1\},0.9)$; $a\ge 3$ is assumed (the case of $a<3$ can be treated analogously). To estimate the minimum $\sigma(S)$ in this case, we can assume that one corner of $S$ touches the $x$-axis and point $(2,0.9)$ is on an edge of $S$. If point $(1,1)$ is in $S$, then $\sigma(S;L_3)+\sigma(S;Q)\ge 0.5$ and $\sigma(S)\ge d+0.5c+0.5>1$ by Lemma 5. Assume that $(1,1)$ is not in $S$. Let $p'=(1,1-c')$ be the crossing point of line segment $[(1,0.9),(1,1)]$ and an edge of $S$, where line segment $[p',(1,1)]$ is not covered by $S$. Note that $\sigma(S;L_1)=0.5c$ and $\sigma(S;L_3)+\sigma(S;Q)=0.5-0.5c'$. Then $\sigma(S)\ge d+0.5c+0.5-0.5c'$, which is greater than 1 by Lemma 6.

### 5.6 Case-7

Finally we consider the case where the center of $S$ belongs to the rectangle $[1,a-1]\times[0,1]$, and line $y=1$ intersects two nonadjacent edges $e_1$ and $e_2$ of $S$. Let $L'$ be the line segment obtained as the intersection of line $y=1$ and $S$, and $c$ be the length of $L'$. Then $c\ge\lambda>1$ since $y=1$ intersects two nonadjacent edges of $S$. If $L'$ is contained in $L_1$ (i.e., $S$ contains none of $(0.9,1),(a-0.9,1)\in Q$) and $S$ contains a point in $P$, then $\sigma(S)\ge 0.5c+0.5>1$.

We next consider the case where $S$ contains one of $(0.9,1),(a-0.9,1)\in Q$. If $S$ contains both $(0.9,1)$ and $(a-0.9,1)$, then $L_1$ is entirely contained in $S$, implying $\sigma(S)\ge\sigma(S;R^*)+(0.45+0.05)\times 2>1$. Assume that $S$ contains $(0.9,1)$ but not $(a-0.9,1)$ (the other case can be treated analogously). By Lemma 7(iii), $S$ contains $(2,0.9)$ or $(1,0.9)$. In any case, $S$ contains line segment $[(0.9,1),(1,1)]$ and point $(2,0.9)$ (or line segment $[(1,0.9),(1,1)]$), indicating $\sigma(S)\ge\sigma(S;R^*)+0.5+0.5>1$.

We finally consider the case where $L'$ is contained in $L_1$ and $S$ contains no point in $P$. Then by Lemma 7(iii) $S$ contains $(1,0.9)$ or $(a-1,0.9)$; We assume that $(1,0.9)$ is in $S$ (the other case can be treated analogously). If $S$ contains $(1,1)$, then it also contains line segment $[(1,0.9),(1,1)]$ and satisfies $\sigma(S)\ge 0.5c+0.5>1$. Hence the remaining case is that $S$ contains $(1,0.9)$ but none of $(1,1)$ and $(\min\{2,a-1\},0.9)$ (see Fig. 6). To estimate the minimum $\sigma(S)$ in this case, we can assume that $S$ touches the $x$-axis (allowing it to violate the condition that $y=1$ intersects two nonadjacent edges of $S$). Now $y=1$ intersects two adjacent edges of $S$ and this case has already been discussed in Case-6.

**Figure 6:** Illustration for the case where $S$ contains $(1,0.9)$ but none of $(1,1)$ and $(\min\{2,a-1\},0.9)$ in Case-7. [View the diagram (PDF, p. 12).](../downloads/nagamochi-2005.pdf#page=12)

This completes the proof of Lemma 1.

## 6 Concluding Remarks

In this paper, we have established a nontrivial upper bound on the number of unit squares that can be packed into a rectangle with given side lengths. With this bound, we have derived a stronger lower bound on $s(N)$ and determined that $s(n^2-1)=s(n^2-2)=n$ for all integers $n\ge 2$. Our unavoidable set $U$ can be seen as a modification of the entire area $R$ so that the total score becomes less than the area of $R$ by replacing the boundary part of $R$ with a set of points and line segments with appropriate scores. This technique can be easily applied to the problem of packing unit squares into other types of convex polygons such as regular $k$-gons ($k\ge 3$) by modifying the definition of endpoints in $Q$ and their scores.

## Acknowledgment

We would like to express our gratitude to the anonymous referees whose suggestions contributed to improving the written style. This research was partially supported by a Scientific Grant in Aid from the Ministry of Education, Culture, Sports, Science and Technology of Japan.

## References

- [1] H. T. Croft, K. J. Falconer, and R. K. Guy, Unsolved Problems in Geometry, Springer Verlag, Berlin (1991) 108–114.
- [2] P. Erdős and R. L. Graham, On packing squares with equal squares, J. Combin. Theory Ser. A, 19 (1975) 119–123.
- [3] E. Friedman, Packing unit squares in squares: A survey and new results, The Electronic Journal of Combinatorics, Dynamic Surveys (#DS7), (2000).
- [4] F. Göbel, Geometrical packing and covering problems, in Packing and Covering in Combinatorics, A. Schrijver (ed.), Math Centrum Tracts, 106 (1979) 179–199.
- [5] M. J. Kearney and P. Shiu, Efficient packing of unit squares in a square, The Electronic Journal of Combinatorics, 9 (2002) (#R14).

## Transcription notes

These notes distinguish apparent source errors from extraction errors. The statements above reproduce the printed paper; these notes are editorial, not part of the paper.

[^section]: The introduction on PDF p. 2 says concluding remarks are in “section 5”; the actual heading is section 6.
[^avoidable]: PDF p. 3 prints “an avoidable set” in this sentence, despite defining and using an unavoidable set.
[^coordinates]: PDF p. 4 prints $(i,a-0.9)$ and $(b-0.9,j)$ in $P$. Those coordinates appear to interchange $a$ and $b$ relative to the enclosing rectangle $[0,a]\times[0,b]$. They are retained here as printed.
[^parenthesis]: The unmatched closing parenthesis after $\sqrt{2}-1$ in the proof of Lemma 3 is present on PDF p. 6.
[^factorization]: Lemma 3 on PDF p. 6 prints the displayed factorization exactly as transcribed. It does not equal the preceding definition of $f$ as written: at $t=0$, the two polynomials have values $1.5-\sqrt{2}$ and $(1-\sqrt{2})^2$, respectively. No correction to the paper's polynomial has been substituted.
[^expansion]: Both occurrences of $2\bar c+x^2$ in the proof of Lemma 5 are printed that way on PDF p. 7. Expanding $(\bar c+x)^2-\bar c^2$ would instead give $2\bar c x+x^2$.
[^lemma6]: The proof of Lemma 6 on PDF p. 8 uses $t$ without introducing it there, and prints $c=(t+t^2)/(1+t)$ followed by $c=1-d$. With the printed $d=t(1-t)/(1+t)$ these identities do not agree in general. Both are retained, rather than replacing them with an inferred correction.
[^reference]: The proof of Lemma 7(iii) on PDF p. 10 refers to “(i) and (iii)” as printed, including the self-reference.
