> **Source:** Shuang Wang, Tian Dong, Jiamin Li, *A New Result on Packing Unit Squares into a Large Square*, arXiv:1603.02368 (2016). https://arxiv.org/abs/1603.02368
>
> **License:** Copyright held by the author(s). Distributed by arXiv under the arXiv non-exclusive distribution license 1.0 (http://arxiv.org/licenses/nonexclusive-distrib/1.0/), which grants rights to arXiv only.
>
> Machine conversion of [`../downloads/wang-dong-li-2016.pdf`](../downloads/wang-dong-li-2016.pdf) with pymupdf4llm. Formulas, figures, and tables are often garbled; cite and check the PDF.

A New Result on Packing Unit Squares into a Large Square 

Shuang Wang<sup>a</sup> , Tian Dong<sup>∗,a,1</sup> , Jiamin Li<sup>a</sup> 

> aSchool of Mathematics, Jilin University, Changchun, Jilin 130012, China 

# Abstract 

In their 2009 note: Packing equal squares into a large square, Chung and Graham proved that the wasted area of a large square of side length x is O x<sup>(3+</sup> √2)/7 log x after maximum number of non-overlapping unit squares � � are packed into it, which improved the earlier results of Erd˝os-Graham and Karabash-Soifer. Here we further improve the result to O(x<sup>5/8</sup> ) that also leads to an improvement of the bound for the dual problem: finding the minimum number of unit squares needed for covering the large square, from x<sup>2</sup> + O �x<sup>(3+</sup> √2)/7 log x� to x<sup>2</sup> + O(x<sup>5/8</sup> ). Key words: packing, covering, wasted area, Taylor’s formula 

# 1. Introduction 

In 1975, Erd˝os and Graham [1] investigated the problem of packing a square of side length x with as many non-overlapping unit squares as possible. In other words, the wasted area should be as small as possible. From then on, 5 the problem have already been well studied in the literature [2, 3, 4, 5, 6, 7, 8], in which [2, 5, 6] focus on the case when x is large enough. Following [5], we call the problem Packing Waste Problem. Also, there is a dual problem, called 

> ∗Corresponding author 

> Email addresses: wangshuang@jlu.edu.cn (Shuang Wang), dongtian@jlu.edu.cn (Tian Dong), jmli@jlu.edu.cn (Jiamin Li) 

> 1Tian Dong was supported by National Natural Science Foundation of China under Grant No. 11101185 and 11171133. 

Preprint submitted to Journal of Combinatorial Theory, Series A 

July 1, 2021 

Covering Waste Problem in [5], which is concerned with covering the square with minimium number of unit squares[5, 6, 9, 10, 11, 12]. 

10 Erd˝os and Graham obtained the first estimation of Packing Waste Problem as O(x<sup>7/11</sup> ) [1]. Later, D. Karabash and A. Soifer in [9] gave the estimation of Covering Waste Problem as O(x<sup>2/3</sup> ) that was improved in [5] to O(x<sup>7/11</sup> ). In 2009, Chung and Graham [6] found the best previous bound O x<sup>(3+</sup> √2)/7 log x � � for both problems. 

15 In this paper we use basic analysis tools to improve the result of Chung and Graham to O(x<sup>5/8</sup> ) also for both problems. 

# 2. Preliminary 

Let A be a closed planar region and S(A) the area of it. We define two functions 



where Aλ ⊂ A is a union set of non-overlapping unit squares, and A<sup>′</sup> λ<sup>⊃A</sup> is a union set of unit squares (non-overlapping is not necessary). Specially, 20 when A is a square of side length x, we denote W (A), W<sup>′</sup> (A) as W (x), W<sup>′</sup> (x) respectively. 

To our opinion, the basic task of Packing or Covering Waste Problem is packing or covering a strip of non-integer width [6], say m. Basic idea for packing a strip [6] is to pack stacks of non-overlapping unit squares of height 25 ⌈m⌉ into the strip as close to being orthogonal as possible (see Fig. 1), namely minimize the angle θ in Fig. 1 which satisfies 



Let r = m −⌊m⌋. Obviously when r = 0, θ = 0 trivially. Otherwise, we let θ = αm<sup>β</sup> + o(m<sup>β</sup> ). By comparing with the constant term of (1), we have 



2 



<!-- Start of picture text -->
θ<br>⌈m⌉ m<br>1 1<br><!-- End of picture text -->

Figure 1: Packing a strip of width m. 

Similarly, as shown in Fig. 2, we also use stacks of unit squares of height ⌈m⌉ (hereafter we will call the stacks as rectangles of size 1 ×⌈m⌉ for simplicity) to cover the strip, then angle θ<sup>′</sup> in Fig. 2 satisfies 



We also have θ<sup>′</sup> = 0 when r = 0. If not, then 



Note that when m →∞, θ and θ<sup>′</sup> are less than √2 m<sup>−1/2</sup> . 



<!-- Start of picture text -->
θ ′<br>⌈m⌉ m<br>1 1<br><!-- End of picture text -->

Figure 2: Covering a strip of width m. 

30 

3 

# 3. Packing Waste Problem 

In this section, we will present our main result on Packing Waste Problem in Theorem 1. For the proof of it, three types of basic shapes are introduced as follows. 

- 35 Type 1 shape Rectangle T1 has a length x and width x<sup>′</sup> (see subfigure (a) of Fig. 3) satisfying x<sup>3/4</sup> ≤ x<sup>′</sup> ≤ cx with c ≤ 7 a constant. 

   - Type 2 shape Trapezoid T2 has a height of x, a top edge of length x<sup>′</sup> (see subfigure (b) of Fig. 3) satisfying x<sup>′</sup> ∼ 2x<sup>1/2</sup> and the angle θ between the right-hand side and a vertical line satisfying 0 < θ < √2x<sup>−1/2</sup> . 

- 40 Type 3 shape Trapezoid T3 has a height h ∼<sup>1</sup> 2<sup>x1/2and a top edge of length a</sup> (see subfigure (c) of Fig. 3) where a = ⌊x<sup>1/3</sup> +√2 x<sup>1/6</sup> ⌋ is an exact integer. The angle θ between the right-hand side and a vertical line satisfies 0 < θ < √2x<sup>−1/2</sup> . 



<!-- Start of picture text -->
a<br>x ′<br>θ θ<br>h<br>x ′ x<br>x<br>(a) Type 1 shape. (b) Type 2 shape. (c) Type 3 shape.<br><!-- End of picture text -->

Figure 3: Three types of basic shapes. 

The proof of Theorem 1 will be completed by an induction based on effective 45 packings of these shapes. 

Theorem 1. Keep the notations above. Then 



4 

(iii) W (T3) ≤ (<sup>19</sup> 4<sup>+7</sup> 4 √2)x<sup>1/3</sup> . 

50 Specially, when T1 is a square of side length x, then W (x) ≤ (16√2 + 38)x<sup>5/8</sup> . 

Proof. (i) We partition Type 1 rectangle T1 into a rectangle S1 of size m1 × (x− m2), a rectangle S2 of size m2 × x<sup>′</sup> , and an integer-sided rectangle T1<sup>′, where</sup> m1, m2 ∼ m = x<sup>3/4</sup> , as shown in Fig. 4. It is easy to see that T1<sup>′can be perfectly</sup> packed, that is W (T1<sup>′)=0.Next,wepackS1andS2withrectanglesofsize</sup> 55 1 × ⌈m1⌉ and 1 × ⌈m2⌉ respectively. Finally, only four regions T2i, i = 1, 2, 3, 4, at each end of S1 and S2, left unfilled which clearly belong to Type 2 with height about m, a top edge of length m<sup>′</sup> ∼ 2m<sup>1/2</sup> , and θ < √2m<sup>−1/2</sup> . 



<!-- Start of picture text -->
T23<br>m1 T21 S1 T22<br>S2<br>T1 ′<br>T24<br>T1 m2<br><!-- End of picture text -->

Figure 4: Packing Type 1 rectangle. 

Applying (ii), the wasted area 



Specially, when T1 is a square of side length x, W (x) ≤ (16√2 + 38)x<sup>5/8</sup> . 60 (ii) Now we partition the Type 2 trapezoid T2 into rectangles A1, · · · , As 

5 

and Type 3 trapezoids B1, · · · , Bs (see Fig. 5). Each Bi has height h ∼<sup>1</sup> 2<sup>x1/2</sup> and top edge of length integer a. Thus, s ∼ 2x<sup>1/2</sup> . 



<!-- Start of picture text -->
x ′ − a a<br>h A1 B1<br>h A2 B2<br>θ<br>h As Bs<br><!-- End of picture text -->

Figure 5: Packing Type 2 trapezoid. 

Let ai be the width of Ai. Then we have x<sup>1/2</sup> < ai < (2 + √2)x<sup>1/2</sup> , 2h < ai < 2(2 + √2)h. From (i), we obtain W (Ai) = O(h<sup>5/8</sup> ) = O(x<sup>5/16</sup> ), hence 



Further, (iii) implies that 



which leads to the wasted area of T2 



(iii) We will partition the Type 3 trapezoid T3 into rectangles C0, · · · , Ct, D0, · · · , Dt and F1, triangles E0, · · · , Et with height h1 = ⌊<sup>x</sup> tan<sup>−1/</sup> θ<sup>6⌋andF2with</sup> height h2 satisfying 0 ≤ h2 < h1, as illustrated in Fig. 6. Here t satisfies 



where r<sup>′</sup> is the decimal part of<sup>x</sup> tan<sup>−1/</sup> θ<sup>6.ThewidthofCk,denotedbyck,isset</sup> to be ⌊x<sup>1/3</sup> + √2 x<sup>1/6</sup> ⌋−⌊x<sup>1/3</sup> + (√2 − k)x<sup>1/6</sup> ⌋, and therefore dk, the width of 

6 

65 Dk, equals to ⌊x<sup>1/3</sup> + (√2 − k)x<sup>1/6</sup> ⌋ + kh1 tan θ, k = 0, · · · , t. Note that when 

h1 > h, then the number of Dk is 0, but the result still holds. 



<!-- Start of picture text -->
h1<br>C0 D0<br>E0<br>h1<br>C1 D1<br>E1<br>Et<br>θ<br>h1<br>Ct Dt<br>F2<br>h2<br>F1<br><!-- End of picture text -->

Figure 6: Packing Type 3 trapezoid. 

1) Obviously, each Ck can be packed perfectly with unit squares, thus 



2) It is easy to see that each Ek can not be packed with unit squares. Thus 



3) We will estimate W (<sup>�t</sup> k=0<sup>Dk) as follows.Since d0is an integer, W(D0) =</sup> 0. For k = 1, · · · , t, 0 < kh1 tan θ < 21<sup>x1/2 tan θ<1impliesthat⌈dk⌉=</sup> ⌊x<sup>1/3</sup> + (√2 − k)x<sup>1/6</sup> ⌋ + 1. Let rk be the decimal part of x<sup>1/3</sup> + (√2 − k)x<sup>1/6</sup> . 70 Then 



Next, we will pack Dk with rectangles of size 1 × ⌈dk⌉ and estimate αk more accurately than before. By (1), we obtain 



7 

Substitute (4) into (3), we have 

(x<sup>1/3</sup> +(√2 − k)x<sup>1/6</sup> − rk)(1 − cos αk) = cos αk +sin αk − kx<sup>−1/6</sup> + kr<sup>′</sup> tan θ. (5) 

Substitute Taylor’s formulae for cos αk, sin αk, 



into (5) and set αk = lk1x<sup>−1/6</sup> + lk2x<sup>−1/3</sup> + lk3x<sup>−1/2</sup> + o(x<sup>−1/2</sup> ). Since 0 < kr<sup>′</sup> tan θ < x<sup>−1/3</sup> , we set kr<sup>′</sup> tan θ = γkx<sup>−1/3</sup> + o(x<sup>−1/3</sup> ), it follows that 0 ≤ γk < 1. Comparing the coefficients of terms x<sup>0</sup> and x<sup>−1/6</sup> , on both sides of (5), we have 



Since 0 ≤ k <<sup>1</sup> 2<sup>x2/3 tan θ<</sup> √22<sup>x1/6,we set k= βkx1/6 +o(x1/6),it follows that</sup> √2 0 ≤ βk < 2<sup>.Comparingthecoefficientsoftermsx−1/3,onbothsidesof(5),</sup> we have 



Hence |αk − αk−1| ≤ 3(1 + √2)x<sup>−1/2</sup> , k = 2, · · · , t. 

75 We pack Dk as follows. First, we leave a Type 2 trapezoid D11 at the top of D1. Second, for k = 2, · · · , t, we pack Dk−1 with rectangles of size 1 × ⌈dk−1⌉ when bk ≥ cos α1k−1<sup>.Ifnot,wepackDkwithrectanglesofsize1 × ⌈dk⌉(see</sup> Fig. 7). When αk−1 ≥ αk, the wasted region between Dk−1 and Dk consists of a triangle Xk1 and trapezoids Xk2, Xk3. The case of αk−1 < αk can be treated 80 in similar fashion. Last, we leave Type 2 trapezoid Dt1 at the bottom of Dt. 

80 

The total wasted area of both ends of rectangles of size 1×⌈dk⌉, k = 1, · · · , t, is less than<sup>�t</sup> k=1<sup>h1 · 2 ·1</sup> 2<sup>· 12 tan αk<</sup> √22<sup>x1/3.By(ii),W(D11) + W(Dt1)≤</sup> O(d<sup>5</sup> 1<sup>/6</sup> ) + O(d<sup>5</sup> t<sup>/6</sup> ) = O(x<sup>5/18</sup> ). The wasted area between Dk−1 and Dk is S(Xk1) + S(Xk2) + S(Xk3) <<sup>1</sup> 2<sup>(x1/3)2 · 3(1 +</sup> √2)x<sup>−1/2</sup> +<sup>1</sup> 2<sup>(1 + 1 +</sup> √2)x<sup>1/6</sup> + O(x<sup>1/6</sup> )O(x<sup>−1/6</sup> ) ≤ ( 2<sup>5+ 2</sup> √2)x<sup>1/6</sup> , which implies that the total wasted area of these joints is bounded by (<sup>5</sup> 2<sup>+ 2</sup> √2)x<sup>1/6</sup> · t < (<sup>5</sup> 4 √2 + 2)x<sup>1/3</sup> . Thus, 



8 



<!-- Start of picture text -->
Dk−1<br>αk−1<br>Xk 2<br>bk X k3<br>Xk1<br>αk<br>Dk<br><!-- End of picture text -->

Figure 7: The wasted region between Dk−1 and Dk. 

4) At last, we will estimate W (F1) and W (F2). The height of the rectangle F1 satisfies 0 ≤ h2 < min(h, h1), and the width of it, denoted by f1, satisfies f1 ∼ x<sup>1/3</sup> . When 0 ≤ h2 ≤ x<sup>1/3</sup> , we pack ⌊h2⌋× ⌊f1⌋ unit squares into F1, then W (F1) < h2 + f1 < 2x<sup>1/3</sup> . When x<sup>1/3</sup> < h2 ≤ h, we pack F1 with rectangles 85 of size 1 × ⌈f1⌉, as shown in Fig. 8, where F11, F12 are Type 2 trapezoids. Since W (F11) + W (F12) = O(x<sup>5/18</sup> ), the total wasted area of both ends of the rectangles of size 1 × ⌈f1⌉ is less than h · √2x<sup>−1/6</sup> ∼ √22<sup>x1/3,soW(F1)<</sup> O(x<sup>5/18</sup> ) + √22<sup>x1/3<x1/3.Tosumup,W(F1)<2x1/3.WeestimateW(F2)</sup> in two cases, too. When 0 < θ < x<sup>−2/3</sup> , W (F2) < S(F2) <<sup>1</sup> 2<sup>h2 tan θ<</sup> 8<sup>1x1/3.</sup> 90 When x<sup>−2/3</sup> ≤ θ < √2x<sup>−1/2</sup> , W (F2) < S(F2) < 2<sup>1h</sup> 1<sup>2tan θ<1</sup> 2<sup>x1/3.Therefore,</sup> W (F2) < 2<sup>1x1/3whichimpliesW(F) ≤W(F1) + W(F2) <</sup> 2<sup>5x1/3.</sup> 



<!-- Start of picture text -->
f1 F11 ⌈f1⌉ F1 F12<br>h2<br><!-- End of picture text -->

Figure 8: Packing F1 in the case of x<sup>1/3</sup> < h2 ≤ h. 

9 

Now, it follows from 1), 2), 3), 4) that the total wasted area 



which completes the induction step. For x ≤ 100, W (T1) ≤ (1 + c)x. Because c ≤ 7, (1 + c)x<sup>3/8</sup> < 48 < 15√2 + 38 < (15 + c)√2 + 38, W (T1) ≤ (1 + c)x < ((15+ c)√2+38)x<sup>5/8</sup> , the proof of the initial step of the induction is completed. 

- 95 

# 4. Covering Waste Problem 

Similarly, we can obtain the result of Covering Waste Problem. Note that in type 3 shape Trapezoid T3, a top edge of length a is modified, a = ⌊x<sup>1/3</sup> − √2 x<sup>1/6</sup> ⌋. 

Theorem 2. Keep the notations above. Then 

- 100 (i) W<sup>′</sup> (T1) ≤ ((15 + c)√2 + 38)x<sup>5/8</sup> . (ii) W<sup>′</sup> (T2) ≤ (<sup>19</sup> 2<sup>+7</sup> 2 √2)x<sup>5/6</sup> . 

- (iii) W<sup>′</sup> (T3) ≤ (<sup>19</sup> 4<sup>+7</sup> 4 √2)x<sup>1/3</sup> . 

Specially, when T1 is a square of side length x, then W<sup>′</sup> (x) ≤ (16√2 + 38)x<sup>5/8</sup> . 

Proof. 

- 105 (i) This can be proved in a similar argument to the one of (i) of Theorem 1. (ii) This can be proved in a similar argument to the one of (ii) of Theorem 

   1. 

   - (iii) We consider a coverage of Type 3 trapezoid T3 with rectangles Ck, Dk, k = 

   - 1, · · · , t, with height h1 = ⌊<sup>x</sup> tan<sup>−1</sup> θ<sup>/6′ ⌋andarectangleF1withheighth2satisfying</sup> 

- 110 0 ≤ h2 < h1. The width of Ck, denoted by ck, is set to be ⌊x<sup>1/3</sup> − √2 x<sup>1/6</sup> ⌋− ⌊x<sup>1/3</sup> − (√2 + k)x<sup>1/6</sup> ⌋, and therefore the width of Dk, denoted by dk, is equal to ⌊x<sup>1/3</sup> − (√2 + k)x<sup>1/6</sup> ⌋ + kh1 tan θ<sup>′</sup> , k = 1, · · · , t. It is easy to verify that the width of F1, denoted by f1, equals to a + h tan θ<sup>′</sup> and 0 ≤ t < 2<sup>1x2/3 tan θ′.Set</sup> 

10 



Figure 9: Covering Type 3 trapezoid. 

115 





We cover Dk as follows. First, we leave Type 2 trapezoid Dt1 at the bottom of Dt. Second, for k = t, · · · , 2, we cover Dk with rectangles of size 1 × ⌈dk⌉. When rectangles of size 1 × ⌈dk⌉ cover the right lower point of Dk−1, we cover 120 Dk−1 with rectangles of size 1 × ⌈dk−1⌉, as shown in Fig. 10. When αk−1 ≥ αk, there are a triangle Xk1 and trapezoids Xk2, Xk3 between Dk−1 and Dk needed to be solved further in the following. As shown in the figure, bk is the bottom edge of Xk3. The case of αk−1 < αk can be treated similarly. At last, we leave Type 2 trapezoid D11 at the top of D1. 

11 



<!-- Start of picture text -->
αk−1 Dk−1<br>bk<br>Xk2<br>Xk3<br>Xk1<br>αk<br>Dk<br><!-- End of picture text -->

Figure 10: The wasted area between Dk−1 and Dk for covering. 

The total wasted area of both ends of the rectangles of size 1 × ⌈dk⌉, k = 1, · · · , t, is less than<sup>�t</sup> k=1<sup>h1· 2 ·</sup> 2<sup>1· 12 tan αk<</sup> √22<sup>x1/3.By(ii)ofTheorem</sup> 2, W<sup>′</sup> (D11) + W<sup>′</sup> (Dt1) ≤ O(d<sup>5</sup> 1<sup>/6</sup> ) + O(d<sup>5</sup> t<sup>/6</sup> ) = O(x<sup>5/18</sup> ). It is easy to see that ck−1 − ck, the height of Xk2, is an exact integer. Let the bottom edge of Xk2 be b<sup>′</sup> k2<sup>.WecoverXk2withrectanglesofsize(ck−1−ck) × ⌈b′</sup> k2<sup>⌉.Thewasted</sup> area between Dk−1 and Dk is S(Xk1) + W<sup>′</sup> (Xk2) + S(Xk3) <<sup>1</sup> 2<sup>(x1/3)2· 3(1 +</sup> √2)x<sup>−1/2</sup> +<sup>1</sup> 2<sup>(1+1+</sup> √2)x<sup>1/6</sup> +O(x<sup>1/6</sup> )O(x<sup>−1/6</sup> ) ≤ ( 2<sup>5+2</sup> √2)x<sup>1/6</sup> , which implies that the total wasted area of these joints is bounded by ( 2<sup>5+ 2</sup> √2)x<sup>1/6</sup> · t < (<sup>5</sup> 4 √2 + 2)x<sup>1/3</sup> . Thus, 



125 

4)At last, W<sup>′</sup> (F1) + W<sup>′</sup> (F2) <<sup>5</sup> 2<sup>x1/3.Theproofissimilarto4)of(iii)of</sup> Theorem 1. By 1), 2), 3), 4), we obtain the total wasted area 



The proof of the induction step is omitted. 

12 

# References 

130 

- [1] P. Erd¨os, R. L. Graham, On packing squares with equal squares, J. Combin. Theory Ser. A 19 (1975) 119–123. 

- [2] K. F. Roth, R. C. Vaughan, Inefficiency in packing squares with unit squares, J. Combin. Theory Ser. A 24 (1978) 170–186. 

- [3] W. Stromquist, Packing unit squares inside squares i, ii, iii, unpublished manuscripts. 

135 

      - URL http://www.walterstromquist.com/publications.html 

   - [4] W. Stromquist, Packing 10 or 11 unit squares in a square, Electron. J. Combin. 10 (2003) #R8. 

   - [5] D. Karabash, A. Soifer, Note on covering a square with equal squares, Geombinatorics 18 (2008) 13–17. 

- 140 [6] F. Chung, R. Graham, Packing equal squares into a large square, J. Combin. Theory Ser. A 116 (2009) 1167–1175. 

   - [7] E. Friedman, Packing unit squares in squares: A survey and new results, Electron. J. Combin. (2009) #DS7. 

145 

   - [8] W. Bentz, Optimal packings of 13 and 46 unit squares in a square, Electron. J. Combin. 17 (2010) #R126. 

   - [9] D. Karabash, A. Soifer, A sharp upper bound for cover-up squares, Geombinatorics 16 (2006) 219–226. 

   - [10] A. Soifer, Covering a square of side n + ε with unit squares, J. Combin. Theory Ser. A 113 (2006) 380–388. 

- 150 [11] E. Friedman, D. Paterson, Covering squares with unit squares, Geombinatorics 15 (2006) 130–137. 

   - [12] J. Januszewski, A note on covering a square of side length 2 + ε with unit squares, Amer. Math. Monthly 19 (2009) 174–178. 

13 

