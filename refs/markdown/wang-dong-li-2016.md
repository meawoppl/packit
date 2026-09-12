> **Source:** Shuang Wang, Tian Dong, Jiamin Li, *A New Result on Packing Unit Squares into a Large Square*, arXiv:1603.02368 (2016). https://arxiv.org/abs/1603.02368
>
> **License:** Copyright held by the author(s). Distributed by arXiv under the arXiv non-exclusive distribution license 1.0 (http://arxiv.org/licenses/nonexclusive-distrib/1.0/), which grants rights to arXiv only.
>
> Math transcription checked against [`../downloads/wang-dong-li-2016.pdf`](../downloads/wang-dong-li-2016.pdf). Inline and displayed equations were restored from the original. Figure captions link to the source pages; consult the PDF for diagrams and authoritative wording. Apparent errors printed in the original are retained and identified in the transcription notes below.

A New Result on Packing Unit Squares into a Large Square 

Shuang Wang<sup>a</sup> , Tian Dong<sup>∗,a,1</sup> , Jiamin Li<sup>a</sup> 

> aSchool of Mathematics, Jilin University, Changchun, Jilin 130012, China 

# Abstract 

In their 2009 note: Packing equal squares into a large square, Chung and Graham proved that the wasted area of a large square of side length $x$ is $O\left(x^{(3+\sqrt{2})/7}\log x\right)$ after maximum number of non-overlapping unit squares are packed into it, which improved the earlier results of Erdős-Graham and Karabash-Soifer. Here we further improve the result to $O(x^{5/8})$ that also leads to an improvement of the bound for the dual problem: finding the minimum number of unit squares needed for covering the large square, from $x^2 + O\left(x^{(3+\sqrt{2})/7}\log x\right)$ to $x^2 + O(x^{5/8})$.

Key words: packing, covering, wasted area, Taylor’s formula 

# 1. Introduction 

In 1975, Erdős and Graham [1] investigated the problem of packing a square of side length $x$ with as many non-overlapping unit squares as possible. In other words, the wasted area should be as small as possible. From then on, the problem have already been well studied in the literature [2, 3, 4, 5, 6, 7, 8], in which [2, 5, 6] focus on the case when $x$ is large enough. Following [5], we call the problem Packing Waste Problem. Also, there is a dual problem, called Covering Waste Problem in [5], which is concerned with covering the square with minimium number of unit squares[5, 6, 9, 10, 11, 12].

> ∗Corresponding author 

> Email addresses: wangshuang@jlu.edu.cn (Shuang Wang), dongtian@jlu.edu.cn (Tian Dong), jmli@jlu.edu.cn (Jiamin Li) 

> 1Tian Dong was supported by National Natural Science Foundation of China under Grant No. 11101185 and 11171133. 

Preprint submitted to Journal of Combinatorial Theory, Series A 

July 1, 2021 

Erdős and Graham obtained the first estimation of Packing Waste Problem as $O(x^{7/11})$ [1]. Later, D. Karabash and A. Soifer in [9] gave the estimation of Covering Waste Problem as $O(x^{2/3})$ that was improved in [5] to $O(x^{7/11})$. In 2009, Chung and Graham [6] found the best previous bound $O\left(x^{(3+\sqrt{2})/7}\log x\right)$ for both problems. 

In this paper we use basic analysis tools to improve the result of Chung and Graham to $O(x^{5/8})$ also for both problems. 

# 2. Preliminary 

Let $A$ be a closed planar region and $S(A)$ the area of it. We define two functions 

```math
\begin{aligned}
W(A) &= S(A) - \sup s(A_\lambda),\\
W'(A) &= \inf s(A'_\lambda) - S(A),
\end{aligned}
```

where $A_\lambda \subset A$ is a union set of non-overlapping unit squares, and $A'_\lambda \supset A$ is a union set of unit squares (non-overlapping is not necessary). Specially, when $A$ is a square of side length $x$, we denote $W(A), W'(A)$ as $W(x), W'(x)$ respectively. 

To our opinion, the basic task of Packing or Covering Waste Problem is packing or covering a strip of non-integer width [6], say $m$. Basic idea for packing a strip [6] is to pack stacks of non-overlapping unit squares of height $\lceil m\rceil$ into the strip as close to being orthogonal as possible (see Fig. 1), namely minimize the angle $\theta$ in Fig. 1 which satisfies 

Equation (1):

```math
\lceil m\rceil\cos\theta + \sin\theta = m.
```

Let $r = m - \lfloor m\rfloor$. Obviously when $r = 0$, $\theta = 0$ trivially. Otherwise, we let $\theta = \alpha m^\beta + o(m^\beta)$. By comparing with the constant term of (1), we have 

```math
\theta = \sqrt{2-r}m^{-1/2} + o(m^{-1/2}).
```

**Figure 1:** Packing a strip of width $m$. [View the diagram (PDF, p. 3).](../downloads/wang-dong-li-2016.pdf#page=3)

Similarly, as shown in Fig. 2, we also use stacks of unit squares of height $\lceil m\rceil$ (hereafter we will call the stacks as rectangles of size $1\times\lceil m\rceil$ for simplicity) to cover the strip, then angle $\theta'$ in Fig. 2 satisfies 

Equation (2):

```math
\lceil m\rceil\cos\theta' - \sin\theta' = m.
```

We also have $\theta' = 0$ when $r = 0$. If not, then 

```math
\theta' = \sqrt{2-r}m^{-1/2} + o(m^{-1/2}).
```

Note that when $m\to\infty$, $\theta$ and $\theta'$ are less than $\sqrt{2}m^{-1/2}$. 

**Figure 2:** Covering a strip of width $m$. [View the diagram (PDF, p. 3).](../downloads/wang-dong-li-2016.pdf#page=3)

# 3. Packing Waste Problem 

In this section, we will present our main result on Packing Waste Problem in Theorem 1. For the proof of it, three types of basic shapes are introduced as follows. 

- **Type 1 shape** Rectangle $T_1$ has a length $x$ and width $x'$ (see subfigure (a) of Fig. 3) satisfying $x^{3/4} \le x' \le cx$ with $c \le 7$ a constant. 

- **Type 2 shape** Trapezoid $T_2$ has a height of $x$, a top edge of length $x'$ (see subfigure (b) of Fig. 3) satisfying $x' \sim 2x^{1/2}$ and the angle $\theta$ between the right-hand side and a vertical line satisfying $0 < \theta < \sqrt{2}x^{-1/2}$. 

- **Type 3 shape** Trapezoid $T_3$ has a height $h \sim \frac{1}{2}x^{1/2}$ and a top edge of length $a$ (see subfigure (c) of Fig. 3) where $a = \lfloor x^{1/3} + \sqrt{2}x^{1/6}\rfloor$ is an exact integer. The angle $\theta$ between the right-hand side and a vertical line satisfies $0 < \theta < \sqrt{2}x^{-1/2}$. 

**Figure 3:** Three types of basic shapes. (a) Type 1 shape. (b) Type 2 shape. (c) Type 3 shape. [View the diagram (PDF, p. 4).](../downloads/wang-dong-li-2016.pdf#page=4)

The proof of Theorem 1 will be completed by an induction based on effective packings of these shapes. 

Theorem 1. Keep the notations above. Then 

(i) $W(T_1) \le ((15+c)\sqrt{2}+38)x^{5/8}$.

(ii) $W(T_2) \le (\frac{19}{2}+\frac{7}{2}\sqrt{2})x^{5/6}$.

(iii) $W(T_3) \le (\frac{19}{4}+\frac{7}{4}\sqrt{2})x^{1/3}$. 

Specially, when $T_1$ is a square of side length $x$, then $W(x) \le (16\sqrt{2}+38)x^{5/8}$. 

Proof. (i) We partition Type 1 rectangle $T_1$ into a rectangle $S_1$ of size $m_1 \times (x - m_2)$, a rectangle $S_2$ of size $m_2 \times x'$, and an integer-sided rectangle $T_1'$, where $m_1, m_2 \sim m = x^{3/4}$, as shown in Fig. 4. It is easy to see that $T_1'$ can be perfectly packed, that is $W(T_1') = 0$. Next, we pack $S_1$ and $S_2$ with rectangles of size $1 \times \lceil m_1\rceil$ and $1 \times \lceil m_2\rceil$ respectively. Finally, only four regions $T_{2i}, i = 1, 2, 3, 4$, at each end of $S_1$ and $S_2$, left unfilled which clearly belong to Type 2 with height about $m$, a top edge of length $m' \sim 2m^{1/2}$, and $\theta < \sqrt{2}m^{-1/2}$. 

**Figure 4:** Packing Type 1 rectangle. [View the diagram (PDF, p. 5).](../downloads/wang-dong-li-2016.pdf#page=5)

Applying (ii), the wasted area 

```math
\begin{aligned}
W(T_1) &\le 0 + x\cdot 2\cdot\frac{1}{2}\tan\theta + x'\cdot 2\cdot\frac{1}{2}\tan\theta + \sum_{i=1}^{4} W(T_{2i})\\
&\le (x + x')\cdot\sqrt{2}m^{-1/2} + 4(\frac{19}{2}+\frac{7}{2}\sqrt{2})m^{5/6}\\
&\le ((15+c)\sqrt{2}+38)x^{5/8}.
\end{aligned}
```

Specially, when $T_1$ is a square of side length $x$, $W(x) \le (16\sqrt{2}+38)x^{5/8}$.

(ii) Now we partition the Type 2 trapezoid $T_2$ into rectangles $A_1, \cdots, A_s$ and Type 3 trapezoids $B_1, \cdots, B_s$ (see Fig. 5). Each $B_i$ has height $h \sim \frac{1}{2}x^{1/2}$ and top edge of length integer $a$. Thus, $s \sim 2x^{1/2}$. 

**Figure 5:** Packing Type 2 trapezoid. [View the diagram (PDF, p. 6).](../downloads/wang-dong-li-2016.pdf#page=6)

Let $a_i$ be the width of $A_i$. Then we have $x^{1/2} < a_i < (2+\sqrt{2})x^{1/2}$, $2h < a_i < 2(2+\sqrt{2})h$. From (i), we obtain $W(A_i) = O(h^{5/8}) = O(x^{5/16})$, hence 

```math
W\left(\bigcup_{i=1}^{s} A_i\right) \le \sum_{i=1}^{s} W(A_i) \le O(x^{5/16})\cdot s = O(x^{13/16}).
```

Further, (iii) implies that 

```math
W\left(\bigcup_{i=1}^{s} B_i\right) \le \sum_{i=1}^{s} W(B_i) \le \left(\frac{19}{4}+\frac{7}{4}\sqrt{2}\right)x^{1/3}\cdot s \le (\frac{19}{2}+\frac{7}{2}\sqrt{2})x^{5/6},
```

which leads to the wasted area of $T_2$ 

```math
W(T_2) \le W\left(\bigcup_{i=1}^{s} A_i\right) + W\left(\bigcup_{i=1}^{s} B_i\right) \le \left(\frac{19}{2}+\frac{7}{2}\sqrt{2}\right)x^{5/6}.
```

(iii) We will partition the Type 3 trapezoid $T_3$ into rectangles $C_0, \cdots, C_t$, $D_0, \cdots, D_t$ and $F_1$, triangles $E_0, \cdots, E_t$ with height $h_1 = \lfloor\frac{x^{-1/6}}{\tan\theta}\rfloor$ and $F_2$ with height $h_2$ satisfying $0 \le h_2 < h_1$, as illustrated in Fig. 6. Here $t$ satisfies 

```math
t < h/h_1 = \frac{1}{2}x^{1/2}\bigg/\left(\frac{x^{-1/6}}{\tan\theta} - r'\right) = \frac{x^{2/3}\tan\theta}{2(1 - r'x^{1/6}\tan\theta)} \le \frac{1}{2}x^{2/3}\tan\theta,
```

where $r'$ is the decimal part of $\frac{x^{-1/6}}{\tan\theta}$. The width of $C_k$, denoted by $c_k$, is set to be $\lfloor x^{1/3} + \sqrt{2}x^{1/6}\rfloor - \lfloor x^{1/3} + (\sqrt{2} - k)x^{1/6}\rfloor$, and therefore $d_k$, the width of $D_k$, equals to $\lfloor x^{1/3} + (\sqrt{2} - k)x^{1/6}\rfloor + kh_1\tan\theta$, $k = 0, \cdots, t$. Note that when $h_1 > h$, then the number of $D_k$ is 0, but the result still holds. 

**Figure 6:** Packing Type 3 trapezoid. [View the diagram (PDF, p. 7).](../downloads/wang-dong-li-2016.pdf#page=7)

1) Obviously, each $C_k$ can be packed perfectly with unit squares, thus 

```math
W\left(\bigcup_{k=0}^{t} C_k\right) = \sum_{k=0}^{t} W(C_k) = 0.
```

2) It is easy to see that each $E_k$ can not be packed with unit squares. Thus 

```math
W\left(\bigcup_{k=0}^{t} E_k\right) = \sum_{k=0}^{t} \frac{1}{2}h_1^2\tan\theta \le \frac{1}{4}x^{1/3}.
```

3) We will estimate $W(\bigcup_{k=0}^{t} D_k)$ as follows. Since $d_0$ is an integer, $W(D_0) = 0$. For $k = 1, \cdots, t$, $0 < kh_1\tan\theta < \frac{1}{2}x^{1/2}\tan\theta < 1$ implies that $\lceil d_k\rceil = \lfloor x^{1/3} + (\sqrt{2} - k)x^{1/6}\rfloor + 1$. Let $r_k$ be the decimal part of $x^{1/3} + (\sqrt{2} - k)x^{1/6}$. Then 

Equation (3):

```math
\left\lbrace
\begin{aligned}
d_k &= x^{1/3} + (\sqrt{2} - k)x^{1/6} - r_k + kx^{-1/6} - kr'\tan\theta,\\
\lceil d_k\rceil &= x^{1/3} + (\sqrt{2} - k)x^{1/6} - r_k + 1.
\end{aligned}
\right.
```

Next, we will pack $D_k$ with rectangles of size $1 \times \lceil d_k\rceil$ and estimate $\alpha_k$ more accurately than before. By (1), we obtain 

Equation (4):

```math
\lceil d_k\rceil\cos\alpha_k + \sin\alpha_k = d_k.
```

Substitute (4) into (3), we have 

Equation (5):

```math
(x^{1/3} + (\sqrt{2} - k)x^{1/6} - r_k)(1 - \cos\alpha_k) = \cos\alpha_k + \sin\alpha_k - kx^{-1/6} + kr'\tan\theta.
```

Substitute Taylor’s formulae for $\cos\alpha_k, \sin\alpha_k$, 

```math
\left\lbrace
\begin{aligned}
\cos\alpha_k &= 1 - \frac{1}{2}\alpha_k^2 + \frac{1}{24}\alpha_k^4 + o(\alpha_k^5),\\
\sin\alpha_k &= \alpha_k - \frac{1}{6}\alpha_k^3 + o(\alpha_k^4),
\end{aligned}
\right.
```

into (5) and set $\alpha_k = l_{k1}x^{-1/6} + l_{k2}x^{-1/3} + l_{k3}x^{-1/2} + o(x^{-1/2})$. Since $0 < kr'\tan\theta < x^{-1/3}$, we set $kr'\tan\theta = \gamma_k x^{-1/3} + o(x^{-1/3})$, it follows that $0 \le \gamma_k < 1$. Comparing the coefficients of terms $x^0$ and $x^{-1/6}$, on both sides of (5), we have 

```math
\alpha_k = \sqrt{2}x^{-1/6} + 0\cdot x^{-1/3} + l_{k3}x^{-1/2} + o(x^{-1/2}).
```

Since $0 \le k < \frac{1}{2}x^{2/3}\tan\theta < \frac{\sqrt{2}}{2}x^{1/6}$, we set $k = \beta_k x^{1/6} + o(x^{1/6})$, it follows that $0 \le \beta_k < \frac{\sqrt{2}}{2}$. Comparing the coefficients of terms $x^{-1/3}$, on both sides of (5), we have 

```math
\alpha_k = \sqrt{2}x^{-1/6} + 0\cdot x^{-1/3} + \frac{r_k + \gamma_k - \frac{1}{6}\beta_k - \frac{5}{6}}{\sqrt{2}(1 - \beta_k)}x^{-1/2} + o(x^{-1/2}).
```

Hence $\vert\alpha_k - \alpha_{k-1}\vert \le 3(1+\sqrt{2})x^{-1/2}, k = 2, \cdots, t$. 

We pack $D_k$ as follows. First, we leave a Type 2 trapezoid $D_{11}$ at the top of $D_1$. Second, for $k = 2, \cdots, t$, we pack $D_{k-1}$ with rectangles of size $1 \times \lceil d_{k-1}\rceil$ when $b_k \ge \frac{1}{\cos\alpha_{k-1}}$. If not, we pack $D_k$ with rectangles of size $1 \times \lceil d_k\rceil$ (see Fig. 7). When $\alpha_{k-1} \ge \alpha_k$, the wasted region between $D_{k-1}$ and $D_k$ consists of a triangle $X_{k1}$ and trapezoids $X_{k2}, X_{k3}$. The case of $\alpha_{k-1} < \alpha_k$ can be treated in similar fashion. Last, we leave Type 2 trapezoid $D_{t1}$ at the bottom of $D_t$. 

The total wasted area of both ends of rectangles of size $1\times\lceil d_k\rceil, k = 1, \cdots, t$, is less than $\sum_{k=1}^{t} h_1\cdot 2\cdot\frac{1}{2}\cdot 1^2\tan\alpha_k < \frac{\sqrt{2}}{2}x^{1/3}$. By (ii), $W(D_{11}) + W(D_{t1}) \le O(d_1^{5/6}) + O(d_t^{5/6}) = O(x^{5/18})$. The wasted area between $D_{k-1}$ and $D_k$ is

```math
\begin{aligned}
S(X_{k1}) + S(X_{k2}) + S(X_{k3}) &< \frac{1}{2}(x^{1/3})^2\cdot 3(1+\sqrt{2})x^{-1/2} + \frac{1}{2}(1 + 1 + \sqrt{2})x^{1/6}\\
&\quad + O(x^{1/6})O(x^{-1/6})\\
&\le (\frac{5}{2} + 2\sqrt{2})x^{1/6},
\end{aligned}
```

which implies that the total wasted area of these joints is bounded by $(\frac{5}{2} + 2\sqrt{2})x^{1/6}\cdot t < (\frac{5}{4}\sqrt{2} + 2)x^{1/3}$. Thus, 

```math
W\left(\bigcup_{k=0}^{t} D_k\right) < 0 + \frac{\sqrt{2}}{2}x^{1/3} + O(x^{5/18}) + \left(\frac{5}{4}\sqrt{2}+2\right)x^{1/3} \le \left(\frac{7}{4}\sqrt{2}+2\right)x^{1/3}.
```

**Figure 7:** The wasted region between $D_{k-1}$ and $D_k$. [View the diagram (PDF, p. 9).](../downloads/wang-dong-li-2016.pdf#page=9)

4) At last, we will estimate $W(F_1)$ and $W(F_2)$. The height of the rectangle $F_1$ satisfies $0 \le h_2 < \min(h, h_1)$, and the width of it, denoted by $f_1$, satisfies $f_1 \sim x^{1/3}$. When $0 \le h_2 \le x^{1/3}$, we pack $\lfloor h_2\rfloor \times \lfloor f_1\rfloor$ unit squares into $F_1$, then $W(F_1) < h_2 + f_1 < 2x^{1/3}$. When $x^{1/3} < h_2 \le h$, we pack $F_1$ with rectangles of size $1 \times \lceil f_1\rceil$, as shown in Fig. 8, where $F_{11}, F_{12}$ are Type 2 trapezoids. Since $W(F_{11}) + W(F_{12}) = O(x^{5/18})$, the total wasted area of both ends of the rectangles of size $1 \times \lceil f_1\rceil$ is less than $h\cdot\sqrt{2}x^{-1/6} \sim \frac{\sqrt{2}}{2}x^{1/3}$, so $W(F_1) < O(x^{5/18}) + \frac{\sqrt{2}}{2}x^{1/3} < x^{1/3}$. To sum up, $W(F_1) < 2x^{1/3}$. We estimate $W(F_2)$ in two cases, too. When $0 < \theta < x^{-2/3}$, $W(F_2) < S(F_2) < \frac{1}{2}h^2\tan\theta < \frac{1}{8}x^{1/3}$. When $x^{-2/3} \le \theta < \sqrt{2}x^{-1/2}$, $W(F_2) < S(F_2) < \frac{1}{2}h_1^2\tan\theta < \frac{1}{2}x^{1/3}$. Therefore, $W(F_2) < \frac{1}{2}x^{1/3}$ which implies $W(F) \le W(F_1) + W(F_2) < \frac{5}{2}x^{1/3}$. 

**Figure 8:** Packing $F_1$ in the case of $x^{1/3} < h_2 \le h$. [View the diagram (PDF, p. 9).](../downloads/wang-dong-li-2016.pdf#page=9)

Now, it follows from 1), 2), 3), 4) that the total wasted area 

```math
W(T_3) \le 0 + \frac{1}{4}x^{1/3} + (\frac{7}{4}\sqrt{2}+2)x^{1/3} + \frac{5}{2}x^{1/3} = (\frac{19}{4}+\frac{7}{4}\sqrt{2})x^{1/3}
```

which completes the induction step. For $x \le 100$, $W(T_1) \le (1+c)x$. Because $c \le 7$, $(1+c)x^{3/8} < 48 < 15\sqrt{2} + 38 < (15+c)\sqrt{2} + 38$, $W(T_1) \le (1+c)x < ((15+c)\sqrt{2}+38)x^{5/8}$, the proof of the initial step of the induction is completed. 

# 4. Covering Waste Problem 

Similarly, we can obtain the result of Covering Waste Problem. Note that in type 3 shape Trapezoid $T_3$, a top edge of length $a$ is modified, $a = \lfloor x^{1/3} - \sqrt{2}x^{1/6}\rfloor$. 

Theorem 2. Keep the notations above. Then 

(i) $W'(T_1) \le ((15+c)\sqrt{2}+38)x^{5/8}$.

(ii) $W'(T_2) \le (\frac{19}{2}+\frac{7}{2}\sqrt{2})x^{5/6}$.

(iii) $W'(T_3) \le (\frac{19}{4}+\frac{7}{4}\sqrt{2})x^{1/3}$. 

Specially, when $T_1$ is a square of side length $x$, then $W'(x) \le (16\sqrt{2}+38)x^{5/8}$. 

Proof. 

(i) This can be proved in a similar argument to the one of (i) of Theorem 1.

(ii) This can be proved in a similar argument to the one of (ii) of Theorem 1.

(iii) We consider a coverage of Type 3 trapezoid $T_3$ with rectangles $C_k, D_k, k = 1, \cdots, t$, with height $h_1 = \lfloor\frac{x^{-1/6}}{\tan\theta'}\rfloor$ and a rectangle $F_1$ with height $h_2$ satisfying $0 \le h_2 < h_1$. The width of $C_k$, denoted by $c_k$, is set to be $\lfloor x^{1/3} - \sqrt{2}x^{1/6}\rfloor - \lfloor x^{1/3} - (\sqrt{2} + k)x^{1/6}\rfloor$, and therefore the width of $D_k$, denoted by $d_k$, is equal to $\lfloor x^{1/3} - (\sqrt{2} + k)x^{1/6}\rfloor + kh_1\tan\theta', k = 1, \cdots, t$. It is easy to verify that the width of $F_1$, denoted by $f_1$, equals to $a + h\tan\theta'$ and $0 \le t < \frac{1}{2}x^{2/3}\tan\theta'$. Set $E_k = D_k \setminus T_3, k = 1, \cdots, t, F_2 = F_1 \setminus T_3$ (see Fig. 9), then

```math
\begin{aligned}
T_3 &= \left(\bigcup_{k=1}^{t} C_k\right)\bigcup\left(\bigcup_{k=1}^{t} D_k\right)\bigcup F_1 \setminus \left(\left(\bigcup_{k=1}^{t} E_k\right)\bigcup F_2\right),\\
W'(T_3) &\le \sum_{k=1}^{t} W'(C_k) + \sum_{k=1}^{t} S(E_k) + W'\left(\bigcup_{k=1}^{t} D_k\right) + W'(F_1) + W'(F_2).
\end{aligned}
```

**Figure 9:** Covering Type 3 trapezoid. [View the diagram (PDF, p. 11).](../downloads/wang-dong-li-2016.pdf#page=11)

1) Obviously, $\sum_{k=1}^{t} W'(C_k) = 0$.

2) $\sum_{k=1}^{t} S(E_k) = \sum_{k=1}^{t} \frac{1}{2}h_1^2\tan\theta' \le \frac{1}{4}x^{1/3}$.

3) We estimate $W'(\bigcup_{k=1}^{t} D_k)$ as follows. For $k = 1, \cdots, t$, we want to cover $D_k$ with rectangles of size $1 \times \lceil d_k\rceil$ and estimate $\alpha_k$ more accurately. Similar to (iii) of Theorem 1, we can obtain

```math
\vert\alpha_k - \alpha_{k-1}\vert \le 3(1+\sqrt{2})x^{-1/2}, k = 2, \cdots, t.
```

We cover $D_k$ as follows. First, we leave Type 2 trapezoid $D_{t1}$ at the bottom of $D_t$. Second, for $k = t, \cdots, 2$, we cover $D_k$ with rectangles of size $1 \times \lceil d_k\rceil$. When rectangles of size $1 \times \lceil d_k\rceil$ cover the right lower point of $D_{k-1}$, we cover $D_{k-1}$ with rectangles of size $1 \times \lceil d_{k-1}\rceil$, as shown in Fig. 10. When $\alpha_{k-1} \ge \alpha_k$, there are a triangle $X_{k1}$ and trapezoids $X_{k2}, X_{k3}$ between $D_{k-1}$ and $D_k$ needed to be solved further in the following. As shown in the figure, $b_k$ is the bottom edge of $X_{k3}$. The case of $\alpha_{k-1} < \alpha_k$ can be treated similarly. At last, we leave Type 2 trapezoid $D_{11}$ at the top of $D_1$. 

**Figure 10:** The wasted area between $D_{k-1}$ and $D_k$ for covering. [View the diagram (PDF, p. 12).](../downloads/wang-dong-li-2016.pdf#page=12)

The total wasted area of both ends of the rectangles of size $1 \times \lceil d_k\rceil, k = 1, \cdots, t$, is less than $\sum_{k=1}^{t} h_1\cdot 2\cdot\frac{1}{2}\cdot 1^2\tan\alpha_k < \frac{\sqrt{2}}{2}x^{1/3}$. By (ii) of Theorem 2, $W'(D_{11}) + W'(D_{t1}) \le O(d_1^{5/6}) + O(d_t^{5/6}) = O(x^{5/18})$. It is easy to see that $c_{k-1} - c_k$, the height of $X_{k2}$, is an exact integer. Let the bottom edge of $X_{k2}$ be $b'_{k2}$. We cover $X_{k2}$ with rectangles of size $(c_{k-1} - c_k) \times \lceil b'_{k2}\rceil$. The wasted area between $D_{k-1}$ and $D_k$ is

```math
\begin{aligned}
S(X_{k1}) + W'(X_{k2}) + S(X_{k3}) &< \frac{1}{2}(x^{1/3})^2\cdot 3(1+\sqrt{2})x^{-1/2} + \frac{1}{2}(1+1+\sqrt{2})x^{1/6}\\
&\quad + O(x^{1/6})O(x^{-1/6})\\
&\le (\frac{5}{2}+2\sqrt{2})x^{1/6},
\end{aligned}
```

which implies that the total wasted area of these joints is bounded by $(\frac{5}{2} + 2\sqrt{2})x^{1/6}\cdot t < (\frac{5}{4}\sqrt{2} + 2)x^{1/3}$. Thus, 

```math
W'\left(\bigcup_{k=0}^{t} D_k\right) < 0 + \frac{\sqrt{2}}{2}x^{1/3} + O(x^{5/18}) + \left(\frac{5}{4}\sqrt{2}+2\right)x^{1/3} \le \left(\frac{7}{4}\sqrt{2}+2\right)x^{1/3}.
```

4)At last, $W'(F_1) + W'(F_2) < \frac{5}{2}x^{1/3}$. The proof is similar to 4) of (iii) of Theorem 1.

By 1), 2), 3), 4), we obtain the total wasted area 

```math
W'(T_3) \le 0 + \frac{1}{4}x^{1/3} + \left(\frac{7}{4}\sqrt{2}+2\right)x^{1/3} + \frac{5}{2}x^{1/3} = \left(\frac{19}{4}+\frac{7}{4}\sqrt{2}\right)x^{1/3}.
```

The proof of the induction step is omitted. 

# References 

- [1] P. Erdös, R. L. Graham, On packing squares with equal squares, J. Combin. Theory Ser. A 19 (1975) 119–123. 

- [2] K. F. Roth, R. C. Vaughan, Inefficiency in packing squares with unit squares, J. Combin. Theory Ser. A 24 (1978) 170–186. 

- [3] W. Stromquist, Packing unit squares inside squares i, ii, iii, unpublished manuscripts. URL http://www.walterstromquist.com/publications.html 

- [4] W. Stromquist, Packing 10 or 11 unit squares in a square, Electron. J. Combin. 10 (2003) #R8. 

- [5] D. Karabash, A. Soifer, Note on covering a square with equal squares, Geombinatorics 18 (2008) 13–17. 

- [6] F. Chung, R. Graham, Packing equal squares into a large square, J. Combin. Theory Ser. A 116 (2009) 1167–1175. 

- [7] E. Friedman, Packing unit squares in squares: A survey and new results, Electron. J. Combin. (2009) #DS7. 

- [8] W. Bentz, Optimal packings of 13 and 46 unit squares in a square, Electron. J. Combin. 17 (2010) #R126. 

- [9] D. Karabash, A. Soifer, A sharp upper bound for cover-up squares, Geombinatorics 16 (2006) 219–226. 

- [10] A. Soifer, Covering a square of side $n + \varepsilon$ with unit squares, J. Combin. Theory Ser. A 113 (2006) 380–388. 

- [11] E. Friedman, D. Paterson, Covering squares with unit squares, Geombinatorics 15 (2006) 130–137. 

- [12] J. Januszewski, A note on covering a square of side length $2 + \varepsilon$ with unit squares, Amer. Math. Monthly 19 (2009) 174–178. 

## Transcription notes (not part of the paper)

The text above follows the committed PDF, including these apparent errors in the original. They are kept as printed rather than silently repaired. Page numbers are PDF pages.

- pp. 2–3: both angle asymptotics are printed as $\theta = \sqrt{2-r} m^{-1/2} + o(m^{-1/2})$ (and likewise for $\theta'$). Expanding equation (1) gives $\theta^2 \approx 2(1-r)/m$, so $\sqrt{2(1-r)}$ appears to be meant.
- p. 6: the step ending $\frac{x^{2/3}\tan\theta}{2(1 - r'x^{1/6}\tan\theta)} \le \frac{1}{2}x^{2/3}\tan\theta$ has the inequality reversed, since the denominator is at most $2$; the bound holds only up to a factor $1 + O(x^{-1/3})$.
- p. 8: "Substitute (4) into (3)"; deriving (5) substitutes (3) into (4).
- p. 9: "$W(F) \le W(F_1) + W(F_2)$" uses $F$, which is never defined; $F = F_1 \cup F_2$ appears to be meant.
- p. 12: the covering bound is printed with $\bigcup_{k=0}^{t} D_k$, although the covering $D_k$ are indexed $k = 1, \ldots, t$.
- p. 12: "$c_{k-1} - c_k$, the height of $X_{k2}$" (also in $(c_{k-1} - c_k) \times \lceil b'_{k2} \rceil$). With $c_k$ as defined on p. 10 this is non-positive; $c_k - c_{k-1}$ appears to be meant.

Minor typos in the original ("minimium", "the problem have") are also kept as printed.
