> **Source:** Hiroshi Nagamochi, *Packing Unit Squares in a Rectangle*, Electronic Journal of Combinatorics 12 (2005) #R37. https://doi.org/10.37236/1934
>
> **License:** Copyright held by the author(s). Published by the Electronic Journal of Combinatorics under a non-exclusive publication agreement; no open license was attached (pre-2018 EJC paper).
>
> Machine conversion of [`../downloads/nagamochi-2005.pdf`](../downloads/nagamochi-2005.pdf) with pymupdf4llm. Formulas, figures, and tables are often garbled; cite and check the PDF.

# Packing Unit Squares in a Rectangle 

Hiroshi Nagamochi 

Department of Applied Mathematics and Physics, Kyoto University Sakyo, Kyoto-city, Kyoto 606-8501, Japan `nag@amp.i.kyoto-u.ac.jp` 

Submitted: Sep 29, 2004; Accepted: Jul 8, 2005; Published: Jul 30, 2005. Mathematics Subject Classifications: 05B40, 52C15 

#### **Abstract** 

For a positive integer _N_ , let _s_ ( _N_ ) be the side length of the minimum square into which _N_ unit squares can be packed. This paper shows that, for given real numbers _a, b ≥_ 2, no more than _ab −_ ( _a_ + 1 _−⌈a⌉_ ) _−_ ( _b_ + 1 _−⌈b⌉_ ) unit squares can be packed in any _a_<sup>_′_</sup> _× b_<sup>_′_</sup> rectangle _R_ with _a_<sup>_′_</sup> _< a_ and _b_<sup>_′_</sup> _< b_ . From this, we can deduce that, for any integer _N ≥_ 4, _s_ ( _N_ ) _≥_ min _{⌈√N ⌉,_ � _N −_ 2 _⌊√N ⌋_ + 1 + 1 _}_ . In particular, for any integer _n ≥_ 2, _s_ ( _n_<sup>2</sup> ) = _s_ ( _n_<sup>2</sup> _−_ 1) = _s_ ( _n_<sup>2</sup> _−_ 2) = _n_ holds. 

## **1 Introduction** 

Packing geometric objects such as circles and squares into another object is one of the fundamental problems in combinatorial geometry [1, 2, 4]. For a positive integer _N_ , let _s_ ( _N_ ) be the side length of the minimum square that can contain _N_ unit squares in the plane whose interiors do not overlap. The problem of packing unit squares into a square was initiated by Erd˝os and Graham [2]. They prove that, for a large number _s_ , unit squares can be packed into an _s × s_ square so that the wasted area is _O_ ( _s_<sup>7</sup><sup>_/_11</sup> ). This is surprisingly small compared with the wasted area in the ‘trivial’ packing of _N_ = _n_<sup>2</sup> _− n_ unit squares in an _n × n_ square, where _n_ is an integer more than 1. 

Determining or estimating _s_ ( _N_ ) is posed as one of the unsolved geometric problems listed by Croft et al. [1]. We easily observe that for any positive integer _N_ , _√N ≤ s_ ( _N_ ) _≤⌈√N ⌉_ , and that for any square number _N_ = _n_<sup>2</sup> , _s_ ( _N_ ) = _n_ . It was conjectured that _s_ ( _n_<sup>2</sup> _−n_ ) = _n_ holds for integers _n ≥_ 2 (whenever _n_ is small). For _n ≥_ 17, _s_ ( _n_<sup>2</sup> _−n_ ) _< n_ is demonstrated by an explicit construction (see [3]). Friedman [3] conjectures that, once _s_ ( _n_<sup>2</sup> _− k_ ) = _n_ holds for some integers _n_ and _k_ , _s_ (( _n_ + 1)<sup>2</sup> _− k_ ) = _n_ + 1 holds. Determining _s_ ( _N_ ) for non-square numbers _N_ seems rather difficult. Currently such _s_ ( _N_ ) has been determined only for some limited numbers _N <_ 100 (see [3, 5]). These nontrivial values for _s_ ( _N_ ) are based on lower bounds which are established in a particular way for each _N_ . 

1 

the electronic journal of combinatorics **12** (2005), #R37 

In this paper, we introduce a lower bound on _s_ ( _N_ ) that is systematically constructible for any integer _N ≥_ 4. For two positive real numbers _a_ and _b_ , let _ν_ ( _a, b_ ) denote the maximum number of unit squares that can be packed into the inside of an _a_<sup>_′_</sup> _× b_<sup>_′_</sup> rectangle _R_ with _a_<sup>_′_</sup> _< a_ and _b_<sup>_′_</sup> _< b_ . A trivial upper bound on _ν_ ( _a, b_ ) is _ν_ ( _a, b_ ) _< ab_ . In this paper, we prove the following result. 

In particular, for two integers _a ≥ b ≥_ 2, we see that an _a × b_ rectangle is the smallest rectangle with aspect ratio _a/b_ into which _ab −_ 2 unit squares can be packed. Theorem 1 also provides a new lower bound on _s_ ( _N_ ), determining _s_ ( _N_ ) for infinitely many new numbers _N_ . 

**Theorem 2** (i) _For any positive integer N such that N ∈{n_<sup>2</sup> _, n_<sup>2</sup> _−_ 1 _, n_<sup>2</sup> _−_ 2 _} for some integer n ≥_ 1 _, s_ ( _N_ ) = _n holds._ 



Note that our new lower bound in Theorem 2(ii) is strictly stronger than the trivial lower bound _√N_ . This paper is organized as follows. After deriving Theorem 2 from Theorem 1 in section 2, we define an unavoidable set _U_ in section 3, showing that proving the unavoidability of _U_ implies Theorem 1. In section 5, we present a proof for the unavoidability of _U_ after preparing a series of technical lemmas in section 4. We make concluding remarks in section 5. 

## **2 Proof of Theorem 2** 

This section shows that Theorem 2 follows from Theorem 1. Any square number _N_ = _n_<sup>2</sup> satisfies _s_ ( _N_ ) = _n_ = _√N_ = _⌈√N⌉_ = � _N −_ 2 _⌊√N ⌋_ + 1 + 1 and inequality _√N_ + 2 _≥ ⌈√N ⌉_ . Now assume that _√N_ is not an integer, for which _⌈√N⌉_ = _⌊√N⌋_ + 1 holds. Then we have � _N −_ 2 _⌊√N ⌋_ + 1 + 1 = � _N −_ ( _⌈√N⌉_ )<sup>2</sup> + ( _⌊√N ⌋_ + 1)<sup>2</sup> _−_ 2 _⌊√N ⌋_ + 1 + 1 = � _N_ + 2 _−_ ( _⌈√N⌉_ )<sup>2</sup> + ( _⌊√N ⌋_ )<sup>2</sup> + 1. Hence � _N −_ 2 _⌊√N⌋_ + 1 + 1 _≥⌊√N⌋_ + 1 if and only if _√N_ + 2 _≥⌈√N⌉_ . A positive integer _N_ satisfies _√N_ + 2 _≥⌈√N⌉_ if and only if there is an integer _n_ such that _√N_ + 2 _≥ n ≥ √N_ , i.e., _n_<sup>2</sup> _≥ N ≥ n_<sup>2</sup> _−_ 2. It is known that _s_ (1) = 1 and _s_ (2) = _s_ (3) = _s_ (4) = 2 [4]. Let _N ≥_ 4. We first consider the case where _√N_ + 2 _≥⌈√N ⌉_ . Then by Theorem 1 with _a_ = _b_ = _⌈√N ⌉≥_ 2, we have _ν_ ( _⌈√N ⌉, ⌈√N ⌉_ ) _<_ ( _⌈√N⌉_ )<sup>2</sup> _−_ 2 _≤ N_ . This says that _N_ unit squares cannot be packed in any square with side length less than _⌈√N ⌉_ . Thus, _s_ ( _N_ ) _≥⌈√N ⌉_ . So for any integer _N ∈{n_<sup>2</sup> _, n_<sup>2</sup> _−_ 1 _, n_<sup>2</sup> _−_ 2 _}_ , where _n ≥_ 1 is an integer, we have _s_ ( _N_ ) _≥ n_ = _⌈√N⌉≥ s_ ( _N_ ). This proves (i). 

**Theorem 1** _For real numbers a, b ≥_ 2 _, ν_ ( _a, b_ ) _< ab −_ ( _a_ + 1 _−⌈a⌉_ ) _−_ ( _b_ + 1 _−⌈b⌉_ ) _._ 2 

2 

2 

the electronic journal of combinatorics **12** (2005), #R37 

We next consider the case where _√N_ + 2 _< ⌈√N⌉_ . Let _k_ = _⌊√N⌋≥_ 2 and _α_ = � _N −_ 2 _⌊√N ⌋_ + 1 _−⌊√N ⌋_ + 1. Note that _α_ is a solution to ( _α_ + _k −_ 1)<sup>2</sup> = _N −_ 2 _k_ + 1. Note that _α <_ 1 since _√N_ + 2 _< ⌈√N⌉_ . Hence by Theorem 1 with _a_ = _b_ = _k_ + _α ≥_ 2, we have _ν_ ( _k_ + _α, k_ + _α_ ) _<_ ( _k_ + _α_ )<sup>2</sup> _−_ 2( _α_ +1 _−⌈α⌉_ ) = ( _k_ + _α_ )<sup>2</sup> _−_ 2 _α_ = ( _k_ + _α−_ 1)<sup>2</sup> +2 _k −_ 1 = _N_ . Therefore, _N_ unit squares cannot be packed in any square with side length less than _k_ + _α_ = � _N −_ 2 _⌊√N⌋_ + 1 + 1. Thus, _s_ ( _N_ ) _≥_ � _N −_ 2 _⌊√N ⌋_ + 1 + 1. Furthermore, we see that � _N −_ 2 _⌊√N ⌋_ + 1 + 1 _>_ � _N −_ 2 _√N_ + 1 + 1 = _√N_ . This proves (ii). 

## **3 Unavoidable Sets** 

The conventional method for deriving a lower bound on _s_ ( _N_ ) [3] is as follows. Suppose that we wish to show _s_ ( _N_ ) _≥ a_ . Let _R_ be a square with side length _less_ than _a_ , and _U_ be a set of some points inside _R_ , where _U_ is called _unavoidable_ if any unit square placed inside _R_ must contain at least one point from _U_ . If we successfully obtain an avoidable set _U_ with _|U| < N_ , then we can conclude that _|U|_ + 1 unit squares cannot be packed inside _R_ , i.e., _s_ ( _N_ ) _≥ s_ ( _|U|_ + 1) _≥ a_ . For example, let _N_ = 2. Take a square _R_ with side length less than _a_ = 2. Then we easily see that _U_ consisting of the center of _R_ is unavoidable, and thereby we need a square _R_ with side length at least _a_ = 2 to pack two unit squares, i.e., _s_ (2) _≥_ 2. An unavoidable set _U_ with _|U| < N_ over a smaller square _R_ provides a better lower bound on _s_ ( _N_ ). Only for few integers _N <_ 100, have such unavoidable sets been constructed to obtain nontrivial lower bounds on _s_ ( _N_ ). However, these constructions are not systematic in terms of _N_ , providing no general lower bound on _s_ ( _N_ ) for large _N_ . 

In this paper, we use not only points but also other geometric objects such as line segments and rectangles to define our unavoidable set _U_ . Recall that the trivial lower bound _s_ ( _N_ ) _≥ √N_ follows from the fact that each unit square consumes at least area 1 from the entire square _R_ , where _R_ can be regarded as an unavoidable set from which unit square takes score 1. 

In the _xy_ -plane, a line segment _L_ connecting two points _p_ 1 = ( _x_ 1 _, y_ 1) and _p_ 2 = ( _x_ 2 _, y_ 2) is denoted by _L_ = [ _p_ 1 _, p_ 2] or _L_ = [( _x_ 1 _, y_ 1) _,_ ( _x_ 2 _, y_ 2)]. A rectangle _R_<sup>_′_</sup> with edges parallel with _x_ -, _y_ -axes may be written as [ _x_ 1 _, x_ 2] _×_ [ _y_ 1 _, y_ 2] if the four corners of _R_<sup>_′_</sup> are given by ( _x_ 1 _, y_ 1) _,_ ( _x_ 1 _, y_ 2) _,_ ( _x_ 2 _, y_ 1) and ( _x_ 2 _, y_ 2) for real numbers _x_ 1 _≤ x_ 2 and _y_ 1 _≤ y_ 2. 

To prove _ν_ ( _a, b_ ) _< ab −_ ( _a_ + 1 _−⌈a⌉_ ) _−_ ( _b_ + 1 _−⌈b⌉_ ) for given real numbers _a, b ≥_ 2, we consider a rectangle _R_ = [0 _, a_ ] _×_ [0 _, b_ ] in the _xy_ -plane. Let _U_ consist of a rectangle _R_<sup>_∗_</sup> , four lines _Li_ ( _i_ = 1 _,_ 2 _,_ 3 _,_ 4), a set _Q_ of eight points, and a set _P_ of 2 _⌈a⌉_ + 2 _⌈b⌉−_ 12 points, such that 



3 

the electronic journal of combinatorics **12** (2005), #R37 

(1 _,_ 0 _._ 9) _,_ (1 _, b −_ 0 _._ 9) _,_ ( _a −_ 1 _,_ 0 _._ 9) _,_ ( _a −_ 1 _, b −_ 0 _._ 9) _},_ 

_P_ = _{_ ( _i,_ 0 _._ 9) _,_ ( _i, a −_ 0 _._ 9) _| i_ = 2 _,_ 3 _, . . ., ⌈a⌉−_ 2 _} ∪{_ (0 _._ 9 _, j_ ) _,_ ( _b −_ 0 _._ 9 _, j_ ) _| j_ = 2 _,_ 3 _, . . ., ⌈b⌉−_ 2 _}._ 

See Fig. 1. 



<!-- Start of picture text -->
1+ a - |  - -a  |<br>( a,b )<br>(0, b )<br>Q P P P Q<br>Q Q<br>L 2 - -<br>1+ b - |  b  |<br>P<br>P<br>L 3 R* L 4<br>Q L 1 Q<br>}<br>Q score 0.05 P score 0.5 P P Q<br>score 0.45 score 0.5 0.9<br>(0,0) ( a ,0)<br>}<br><!-- End of picture text -->

Figure 1: An unavoidable set _U_ for a rectangle _R_ = [0 _, a_ ] _×_ [0 _, b_ ]. 

Let _λ >_ 1. We say that _R_ and _U_ are _shrunken_ toward the origin (0 _,_ 0) by factor _λ_<sup>_−_1</sup> if we map each point ( _x, y_ ) in _R_ and _U_ to a new point ( _λ_<sup>_−_1</sup> _x, λ_<sup>_−_1</sup> _y_ ). Let _λ_<sup>_−_1</sup> _R_ and _λ_<sup>_−_1</sup> _U_ respectively denote such _R_ and _U_ shrunken by factor _λ_<sup>_−_1</sup> . 

For a given unit square _S_ inside _λ_<sup>_−_1</sup> _R_ and an object _K ∈{Q, P, L_ 1 _, L_ 2 _, L_ 3 _, L_ 4 _, R_<sup>_∗_</sup> _}_ , we define _score σ_ ( _S_ ; _K_ ) of _S_ by _K_ as follows. 

- _σ_ ( _S_ ; _R_<sup>_∗_</sup> ) =(the area of the intersection of _S_ and _R_<sup>_∗_</sup> ) _×λ_<sup>2</sup> , 

- _σ_ ( _S_ ; _Li_ ) =(the sum of length of the intersection of _S_ and line segment _Li_ ) _×_ 0 _._ 5 _× λ_ , 

- _σ_ ( _S_ ; _Q_ ) =(the number of points in _Q_ contained in _S_ ) _×_ 0 _._ 45, and 

- _σ_ ( _S_ ; _P_ ) =(the number of points in _P_ contained in _S_ ) _×_ 0 _._ 5. 

Define 

_σ_ ( _S_ ) = _σ_ ( _S_ ; _R_<sup>_∗_</sup> ) + _σ_ ( _S_ ; _L_ 1) + _σ_ ( _S_ ; _L_ 2) + _σ_ ( _S_ ; _L_ 3) + _σ_ ( _S_ ; _L_ 4) + _σ_ ( _S_ ; _Q_ ) + _σ_ ( _S_ ; _P_ ) _._ 

Note that the total score from _Li_ ( _i_ = 1 _,_ 2 _,_ 3 _,_ 4) and _Q_ is 2( _a −_ 1 _._ 8) _×_ 0 _._ 5+2( _b −_ 1 _._ 8) _×_ 0 _._ 5+8 _×_ 0 _._ 45 = _a_ + _b_ . Then the total score from _U_ is ( _a−_ 2)( _b−_ 2)+ _a_ + _b_ + _⌈a⌉−_ 3+ _⌈b⌉−_ 3 = _ab −_ ( _a_ + 1 _−⌈a⌉_ ) _−_ ( _b_ + 1 _−⌈b⌉_ ). In what follows, we prove that _U_ is an unavoidable set in the following sense. 

4 

the electronic journal of combinatorics **12** (2005), #R37 

- **Lemma 1** _Any unit square S inside λ_<sup>_−_1</sup> _R satisfies σ_ ( _S_ ) _>_ 1 _._ 2 

We show that Theorem 1 follows from Lemma 1. Assume that _N_<sup>_′_</sup> unit squares are packed inside _λ_<sup>_−_1</sup> _R_ . Each of the _N_<sup>_′_</sup> unit squares has _σ_ ( _S_ ) _>_ 1 by Lemma 1 and the total score of _U_ is _ab−_ ( _a_ +1 _−⌈a⌉_ ) _−_ ( _b_ +1 _−⌈b⌉_ ). Then we have _N_<sup>_′_</sup> _< ab−_ ( _a_ +1 _−⌈a⌉_ ) _−_ ( _b_ +1 _−⌈b⌉_ ) for any factor _λ_<sup>_−_1</sup> _<_ 1, i.e., _ν_ ( _a, b_ ) _< ab −_ ( _a_ + 1 _−⌈a⌉_ ) _−_ ( _b_ + 1 _−⌈b⌉_ ), as required. 

A square _S_ with side length _λ_ is called a _λ × λ_ square. For a notational convenience to prove Lemma 1, we consider packing _λ × λ_ squares with _λ >_ 1 into the original rectangle _R_ = [0 _, a_ ] _×_ [0 _, b_ ], instead of considering _λ_<sup>_−_1</sup> _R_ and _λ_<sup>_−_1</sup> _U_ . In this case, each _Li_ contributes to _σ_ ( _S_ ) by 0.5 per length and _R_<sup>_∗_</sup> by 1 per area while each point in _Q_ (resp., _P_ ) contributes to _σ_ ( _S_ ) by 0.45 (resp., 0.5). It suffices to show that any _λ × λ_ square _S_ with _λ ∈_ (1 _,_ 1 _._ 01] has _σ_ ( _S_ ) _>_ 1 over the original _R_ and _U_ . 

## **4 Technical Lemmas** 

In this section, we prepare some technical lemmas in order to establish a proof of Lemma 1 in the next section. Let _λ ∈_ [1 _,_ 1 _._ 01] for a technical reason to prove the lemmas in this section. 

**Lemma 2** _Let S be a λ × λ square with λ ∈_ [1 _,_ 1 _._ 01] _. For a line L with distance h ∈_ [0 _,_ ( _√_ 2 _−_ 1) _/_ 2) _from the center of S, let c be the length of the intersection of S and L_ ( _see Fig. 2(a)_ ) _. Then c ≥ λ or c >_ 1 _._ **Proof:** Let _L_ intersect edges _e_ 1 and _e_ 2 of _S_ . If _e_ 1 and _e_ 2 are not adjacent, then _c ≥ λ_ . We consider the case where _e_ 1 and _e_ 2 are adjacent. We can assume that _λ_ = 1 to estimate the minimum _c_ . Let _θ_ denote the angle made by _L_ and _e_ 2, where 0 _< θ ≤ π/_ 4 is assumed without loss of generality. Let _t_ = tan( _θ/_ 2), where 0 _< t_ = tan( _θ/_ 2) _≤ √_ 2 _−_ 1 for _θ ∈_ (0 _, π/_ 4]. Then we have 



which is a decreasing function of _h_ for a fixed _t_ . Hence it suffices to show that _f_ ( _h, t_ ) = _−_ 2 _h_ (1 + _t_<sup>2</sup> )<sup>2</sup> + (1 + _t_<sup>2</sup> )(1 + 2 _t − t_<sup>2</sup> ) _−_ 4 _t_ (1 _− t_<sup>2</sup> ) is nonnegative for _h_ = ( _√_ 2 _−_ 1) _/_ 2. We have 



5 

the electronic journal of combinatorics **12** (2005), #R37 



<!-- Start of picture text -->
S<br>e<br>2 e 1<br>h θ c<br>L<br>L<br>c θ<br>e 1 e 2 S<br>h<br>θ<br>(a) (b)<br><!-- End of picture text -->

Figure 2: (a) Illustration for Lemma 2; (b) Illustration for Lemma 3. 

**Proof:** Let _L_ intersect edges _e_ 1 and _e_ 2 of _S_ . We consider the case where _e_ 1 and _e_ 2 are adjacent (otherwise _c ≥ λ_ ). By _h >_ 0 _._ 5, both _e_ 1 and _e_ 2 are not touching the _x_ -axis. We can assume that _λ_ = 1 to estimate the minimum _c_ . Let _θ_ be angle made by _L_ and _e_ 2, where 0 _< θ ≤ π/_ 4 is assumed without loss of generality. Let _t_ = tan( _θ/_ 2), where 0 _< t_ = tan( _θ/_ 2) _≤ √_ 2 _−_ 1) for _θ ∈_ (0 _, π/_ 4]. Then we have 



which is a decreasing function of _h_ for a fixed _t_ . To prove the lemma, it suffices to show that _f_ ( _h, t_ ) = _−_ ( _h −_ 1)(1 + _t_<sup>2</sup> )<sup>2</sup> + 2 _t −_ 2 _t_<sup>2</sup> + 2 _t_<sup>3</sup> _−_ 2 _t_<sup>4</sup> _−_ 2 _t_ + 2 _t_<sup>3</sup> _≥_ 0 for _h_ = _√_ 2 _−_ 0 _._ 5. We see that 



**Proof:** Let _h_ and _ℓ_ be the lengths of the line segments [ _p_ 1 _, v_ ] and [ _v, p_ 2], respectively. Then _c_ = _√h_<sup>2</sup> + _ℓ_<sup>2</sup> and _d_ = _hℓ/_ 2. To prove _c/_ 2 _> d_ , it suffices to show that _h_<sup>2</sup> + _ℓ_<sup>2</sup> _− h_<sup>2</sup> _ℓ_<sup>2</sup> _>_ 0. Since _h, ℓ ∈_ (0 _, λ_ ], we have _h_<sup>2</sup> + _ℓ_<sup>2</sup> _− h_<sup>2</sup> _ℓ_<sup>2</sup> = ( _h − ℓ_ )<sup>2</sup> + _hℓ_ (2 _− hℓ_ ) _≥ hℓ_ (2 _− λ_<sup>2</sup> ) _>_ 0. 2 

6 

the electronic journal of combinatorics **12** (2005), #R37 



<!-- Start of picture text -->
v<br>T<br>e 2 d e 1<br>p 2 p 1<br>c<br>L<br>S<br><!-- End of picture text -->

Figure 3: Illustration for Lemma 4. 

**Lemma 5** _Let S be a λ × λ square with λ ∈_ [1 _,_ 1 _._ 01] _such that one corner of S touches the x-axis and S is entirely above the x-axis, c >_ 0 _be the length of the intersection of S and line L_ : _y_ = 1 _, and d be the area of the triangle enclosed by S and L_ ( _see Fig. 4(a)_ ) _. Then d_ + 0 _._ 5 _c >_ 0 _._ 5 _._ 

**Proof:** Let _θ ∈_ (0 _, π/_ 4] be the angle made by an edge of _S_ and the _x_ -axis, and _t_ = tan( _θ/_ 2). We obtain _d_ = _c_<sup>2</sup> _× t_ (1 _− t_<sup>2</sup> ) _/_ (1 + _t_<sup>2</sup> )<sup>2</sup> . We denote _c_ and _d_ for _λ_ = 1 by _c_ ¯ and _d_<sup>¯</sup> . Then we have 1 _−c_ ¯ = _d_<sup>¯</sup> = ( _t−t_<sup>2</sup> ) _/_ (1+ _t_ ), for which _d_<sup>¯</sup> +0 _._ 5¯ _c_ = 1 _−c_ ¯+0 _._ 5¯ _c_ = 0 _._ 5+0 _._ 5(1 _−c_ ¯) _>_ 0 _._ 5. Now consider the case of _λ >_ 1. Since _λ −_ 1 is small, we can write _c_ = _c_ ¯ + _x_ and _d_ = (¯ _c_ + _x_ )<sup>2</sup> _× t_ (1 _− t_<sup>2</sup> ) _/_ (1 + _t_<sup>2</sup> )<sup>2</sup> = _d_<sup>¯</sup> + (2¯ _c_ + _x_<sup>2</sup> ) _× t_ (1 _− t_<sup>2</sup> ) _/_ (1 + _t_<sup>2</sup> )<sup>2</sup> for some number _x >_ 0. Then _d_ +0 _._ 5 _c_ = _d_<sup>¯</sup> +0 _._ 5¯ _c_ +0 _._ 5 _x_ +(2¯ _c_ + _x_<sup>2</sup> ) _× t_ (1 _− t_<sup>2</sup> ) _/_ (1+ _t_<sup>2</sup> )<sup>2</sup> _≥ d_<sup>¯</sup> +0 _._ 5¯ _c >_ 0 _._ 5. 2 



<!-- Start of picture text -->
d d<br>θ<br>c c { p c (2,0.9)<br>S<br>1 1<br>θ θ<br>x =1 x =2<br>(a) (b)<br>`<br>`<br><!-- End of picture text -->

Figure 4: (a) Illustration for Lemma 5; (b) Illustration for Lemma 6. 

7 

the electronic journal of combinatorics **12** (2005), #R37 

**Lemma 6** _Let S be a λ × λ square with λ ∈_ [1 _,_ 1 _._ 01] _such that one corner of S touches the x-axis and S is entirely above the x-axis. Assume that two adjacent edges e_ 1 _and e_ 2 _of S intersect line L_ : _y_ = 1 _, point_ (1 _,_ 1) _is not in S, point_ (2 _,_ 0 _._ 9) _is on an edge e_ 2 _of S. Let c be the length of the intersection of S and L, d be the area of the triangle enclosed by S and L, and p_<sup>_′_</sup> = (1 _,_ 1 _− c_<sup>_′_</sup> ) _be the crossing point of e_ 1 _and line x_ = 1 ( _see Fig. 4(b)_ ) _. Then d_ + 0 _._ 5 _c_ + 0 _._ 5 _−_ 0 _._ 5 _c_<sup>_′_</sup> _>_ 1 _holds._ 2 

**Proof:** For values _d_ , _c_ , _−c_<sup>_′_</sup> for a _λ × λ_ square _S_ with _λ >_ 1, we can get smaller _d_ , _c_ , _−c_<sup>_′_</sup> choosing a _λ_<sup>_′_</sup> _× λ_<sup>_′_</sup> square _S_ with 1 _≤ λ_<sup>_′_</sup> _< λ_ . Then we only consider the case of _λ_ = 1. Let _θ ∈_ (0 _, π/_ 2] be the angle made by _e_ 1 and _L_ : _y_ = 1. By calculation, we have _d_ = _t_ (1 _− t_ ) _/_ (1 + _t_ ), _c_ = ( _t_ + _t_<sup>2</sup> ) _/_ (1 + _t_ ), and _c_<sup>_′_</sup> = 2 _t_ ( _t_ (1 _− t_ )<sup>2</sup> _−_ 0 _._ 2 _t_ ) _/_ (1 _− t_<sup>2</sup> )<sup>2</sup> . To have _c_<sup>_′_</sup> _>_ 0 (i.e., to keep (1 _,_ 1) outside _S_ ), _t_ (1 _− t_ )<sup>2</sup> _−_ 0 _._ 2 _t >_ 0 (i.e., _t <_ 1 _− √_ 0 _._ 2) must hold. Note that _c_ = 1 _− d_ holds. To prove _d_ + 0 _._ 5 _c_ + 0 _._ 5 _−_ 0 _._ 5 _c_<sup>_′_</sup> _>_ 1, it suffices to show that _d > c_<sup>_′_</sup> , i.e., 



For this, we show _f_ ( _t_ ) = (1 _− t_ )<sup>2</sup> (1 _− t_<sup>2</sup> ) _−_ 2( _t_ (1 _− t_ )<sup>2</sup> _−_ 0 _._ 2 _t_ ) _≥_ 0. We have _f_ ( _t_ ) = (1 _− t_ )<sup>2</sup> (2 _−_ (1 + _t_ )<sup>2</sup> ) + 0 _._ 4 _t_ , which is positive for 0 _< t ≤ √_ 2 _−_ 1. On the other hand, for 0 _._ 41 _< √_ 2 _−_ 1 _< t <_ 1 _− √_ 0 _._ 2 _<_ 0 _._ 56, we have (1 _−_ 0 _._ 41)<sup>2</sup> (2 _−_ (1 + 0 _._ 56)<sup>2</sup> ) + 0 _._ 4 _·_ 0 _._ 41 _>_ 0. This completes the proof of the lemma. 2 

## **5 Proof of Lemma 1** 

Throughout this section, _S_ denotes a _λ × λ_ square with _λ ∈_ (1 _,_ 1 _._ 01] that is entirely contained in a given _a × b_ rectangle _R_ = [0 _, a_ ] _×_ [0 _, b_ ]. We prove that _σ_ ( _S_ ) _>_ 1, from which Lemma 1 follows. We distinguish the following seven cases: 

- **Case-1:** _S_ is contained completely inside _R_<sup>_∗_</sup> = [1 _, a −_ 1] _×_ [1 _, b −_ 1]. 

- **Case-2:** _S_ is not completely contained inside _R_<sup>_∗_</sup> , the center of _S_ is inside _R_<sup>_∗_</sup> , _S_ does not contain any point in _Q_ as its interior point, and there is no line segment _Li ∈ U_ that intersects two nonadjacent edges of _S_ . 

- **Case-3:** The center of _S_ is inside _R_<sup>_∗_</sup> , _S_ does not contain any point in _Q_ as its interior point, and there is a line segment _Li_ that intersects two nonadjacent edges of _S_ . 

- **Case-4:** The center of _S_ is inside _R_<sup>_∗_</sup> , and _S_ contains a point in _Q_ as its interior point. 

- **Case-5:** The center of _S_ belongs to the rectangle [0 _,_ 1] _×_ [0 _,_ 1]. 

- **Case-6:** The center of _S_ belongs to the rectangle [1 _, a−_ 1] _×_ [0 _,_ 1], and line _y_ = 1 intersects two adjacent edges of _S_ . 

8 

the electronic journal of combinatorics **12** (2005), #R37 

- **Case-7:** The center of _S_ belongs to the rectangle [1 _, a−_ 1] _×_ [0 _,_ 1], and line _y_ = 1 intersects two nonadjacent edges of _S_ . 

The case where the center of _S_ belongs to one of the rectangles [ _a −_ 1 _, a_ ] _×_ [0 _,_ 1], [0 _,_ 1] _×_ [ _b −_ 1 _, b_ ] and [ _a −_ 1 _, a_ ] _×_ [ _b −_ 1 _, b_ ] can be treated analogously with Case-5. Also the case where the center of _S_ belongs to one of the rectangles [1 _, a −_ 1] _×_ [ _b −_ 1 _, b_ ], [0 _,_ 1] _×_ [1 _, b −_ 1] and [ _a −_ 1 _, a_ ] _×_ [1 _, b −_ 1] can be treated in a similar way of Cases-6 and 7. 

In Case-1, we easily see that _σ_ ( _S_ ) _≥ σ_ ( _S_ ; _R_<sup>_∗_</sup> ) = _λ_<sup>2</sup> _>_ 1 holds. The rest of the cases will be discussed in the subsequent subsections. 

### **5.1 Case-2** 

In this case, _S_ is not completely contained inside _R_<sup>_∗_</sup> , the center of _S_ is inside _R_<sup>_∗_</sup> , _S_ does not contain any point in _Q_ as its interior point, and there is no line segment _Li ∈ U_ that intersects two nonadjacent edges of _S_ . Then there is a line segment _Li ∈ U_ that intersects two adjacent edges of _S_ , cutting out from _S_ a triangle _Ti_ that is not covered by _R_<sup>_∗_</sup> (see Fig. 5). For each of all those line segments _Li_ , let _di_ be the area of the triangle _Ti_ , and _ci_ be the length of the intersection of _Li_ and _S_ (some of these triangles may be overlapping, as illustrated by _S_ 3 in Fig. 5). By Lemma 4, we have 0 _._ 5 _ci − di >_ 0 for all such _Li_ . This implies that _σ_ ( _S_ ; _Li_ ) = 0 _._ 5 _ci_ compensates the loss _di_ in _σ_ ( _S_ ; _R_<sup>_∗_</sup> ). Thus _σ_ ( _S_ ) is not less than that of a _λ × λ_ square _S_ which is completely contained in _R_<sup>_∗_</sup> . Therefore, _σ_ ( _S_ ) _>_ 1. 



<!-- Start of picture text -->
S 2 S 1<br>S 3<br><!-- End of picture text -->

Figure 5: Illustration for _λ × λ_ squares in Case-2. 

### **5.2 Case-3** 

In this subsection, we consider the case where the center of _S_ is inside _R_<sup>_∗_</sup> , _S_ does not contain any point in _Q_ as its interior point, and there is a line segment _Li_ that intersects two nonadjacent edges of _S_ . The length of the intersection of _Li_ and _S_ is at least _λ >_ 1. Then if there are two such line segments _Li_ and _Li′_ , then _σ_ ( _S_ ) _≥ σ_ ( _S_ ; _Li_ ) + _σ_ ( _S_ ; _Li′_ ) _≥ λ ×_ 0 _._ 5 _×_ 2 _>_ 1. Assume that there is exactly one such line segment _Li_ , which cuts out from _S_ an quadrangle uncovered by _R_<sup>_∗_</sup> . From the above observation using Lemma 4, 

9 

the electronic journal of combinatorics **12** (2005), #R37 

we can assume that there is no other line segment _Lj ∈ U_ that cuts out from _S_ an uncovered triangle _Tj_ . Since the center is in _R_<sup>_∗_</sup> and _σ_ ( _S_ ; _R_<sup>_∗_</sup> ) _≥_ 0 _._ 5 _λ_<sup>2</sup> , we have _σ_ ( _S_ ) _≥ σ_ ( _S_ ; _R_<sup>_∗_</sup> ) + _σ_ ( _S_ ; _Li_ ) _≥_ 0 _._ 5 _λ_<sup>2</sup> + 0 _._ 5 _λ >_ 1. 

### **5.3 Case-4** 

In this case, the center of _S_ is inside _R_<sup>_∗_</sup> , _S_ contains a point in _Q_ as its interior point. We show that this case can be reduced to Case-2. Assume that _S_ contains point (1 _,_ 0 _._ 9) (the case where _S_ contains other point in _Q_ can be treated analogously). To estimate the minimum _σ_ ( _S_ ), we temporarily replace the point (1 _,_ 0 _._ 9) with line segment _L_<sup>_′_</sup> = [(1 _,_ 0 _._ 9) _,_ (1 _,_ 0)], setting the score of _L_<sup>_′_</sup> per length to be 0.5 (note that the total score of _L_<sup>_′_</sup> is 0.45, the same as that of point (1 _,_ 0 _._ 9)). If _S_ contains other points in _Q_ , we replace each of them in a similar manner. With this modification, the score of _S_ never increases and the argument in Case-2 can be applied, indicating _σ_ ( _S_ ) _>_ 1. 

### **5.4 Case-5** 

We start with the following lemma to handle Cases-5, 6 and 7. 

**Lemma 7** _Let S be a λ × λ square with λ ∈_ (1 _,_ 1 _._ 01] _that is entirely contained in R. Assume that the center of S belongs to the rectangle_ [0 _, a_ ] _×_ [0 _,_ 1] _. Then_ 

- (i) _The length c of the intersection of S and line L_ : _y_ = 0 _._ 9 _is more than 1._ 

- (ii) _If the center of S belongs to the square_ [0 _,_ 1] _×_ [0 _,_ 1] _, then S contains three points_ (1 _,_ 1) _and_ (1 _,_ 0 _._ 9) _,_ (0 _._ 9 _,_ 1) _∈ Q as its interior points._ 

- (iii) _S contains at least one point in Q ∪ P ._ 

- (iv) _σ_ ( _S_ ; _R_<sup>_∗_</sup> ) _>_ 0 _._ 

**Proof:** (i) If _L_ intersects two nonadjacent edges of _S_ , then _c ≥ λ >_ 1. Assume that _L_ intersects two adjacent edges of _S_ . If the center is below _L_ then we only have to consider the case where one corner of _S_ touches the _x_ -axis, and in this case _c >_ 1 follows from Lemma 3 with _h_ = 0 _._ 9. In the other case (i.e., the center of _S_ is situated between _L_ and line _y_ = 1), _c >_ 1 holds by Lemma 2 with _h_ = 0 _._ 1. 

(ii) It is known that any unit square inside the first quadrant whose center is in [0 _,_ 1] _×_ [0 _,_ 1] contains the point (1 _,_ 1) (for example, see [3]). Then _S_ contains (1 _,_ 1) as its interior point since _λ >_ 1. We show that _S_ contains (1 _,_ 0 _._ 9) (we can show that _S_ contains (0 _._ 9 _,_ 1) analogously). By (i), _S_ contains one of the points (0 _,_ 0 _._ 9) and (1 _,_ 0 _._ 9). Assume that _S_ contains (0 _,_ 0 _._ 9) but not (1 _,_ 0 _._ 9). This can occur only when one corner of _S_ attaches the _y_ -axis at the point (0 _,_ 0 _._ 9). Let _e_ and _e_<sup>_′_</sup> be the edges of _S_ that are not incident to the point (0 _,_ 0 _._ 9). Since any point on _e_ and _e_<sup>_′_</sup> has distance at least _λ >_ 1 from the (0 _,_ 0 _._ 9), _S_ must contain (1 _,_ 0 _._ 9) as its interior point. 

(iii) Immediate from (i) and (iii). 

10 

the electronic journal of combinatorics **12** (2005), #R37 

(iv) We easily see that _σ_ ( _S_ ; _R_<sup>_∗_</sup> ) _>_ 0 holds from _λ >_ 1 if the center of _S_ belongs to the rectangle [1 _, a −_ 1] _×_ [0 _,_ 1]; _σ_ ( _S_ ; _R_<sup>_∗_</sup> ) _>_ 0 holds from (ii) otherwise. 2 

In Case-5, the center of _S_ belongs to the rectangle [0 _,_ 1] _×_ [0 _,_ 1]. Then by Lemma 7(ii) _S_ contains (1 _,_ 0 _._ 9) _,_ (0 _._ 9 _,_ 1) _∈ Q_ and line segments [(1 _,_ 0 _._ 9) _,_ (1 _,_ 1)] and [(0 _._ 9 _,_ 1) _,_ (1 _,_ 1)], and thereby _σ_ ( _S_ ) _≥ σ_ ( _S_ ; _R_<sup>_∗_</sup> ) + 0 _._ 45 _×_ 2 + 0 _._ 1 _×_ 2 _×_ 0 _._ 5 _>_ 1. 

### **5.5 Case-6** 

In this case, the center of _S_ belongs to the rectangle [1 _, a −_ 1] _×_ [0 _,_ 1], and line _y_ = 1 intersects two adjacent edges _e_ 1 and _e_ 2 of _S_ . Let _L_<sup>_′_</sup> be the line segment obtained as the intersection of line _y_ = 1 and _S_ , _c_ be the length of _L_<sup>_′_</sup> , and _d_ be the area of the triangle _T_ enclosed by _L_<sup>_′_</sup> , _e_ 1 and _e_ 2. 

We first assume that _S_ contains a point in _P_ and none of points (0 _._ 9 _,_ 1) _,_ ( _a−_ 0 _._ 9 _,_ 1) _∈ Q_ . Then _L_<sup>_′_</sup> is contained in _L_ 1. To estimate the minimum of _d_ + 0 _._ 5 _c_ in this case, we can assume that one corner of _S_ touches the _x_ -axis. Assume that triangle _T_ is contained in _R_<sup>_∗_</sup> . Since _S_ contains _T_ , _L_<sup>_′_</sup> and a point in _P_ , we have _σ_ ( _S_ ) _≥ d_ + 0 _._ 5 _c_ + 0 _._ 5 _>_ 1 by Lemma 5. Even if _T_ has an intersection with _L_ 2, _σ_ ( _S_ ) _>_ 1 still holds by applying Lemma 4 to the triangle _T_<sup>_′_</sup> enclosed by _L_ 2 and _S_ . 

We next consider the case where _S_ contains a point in _P_ and at least one of points (0 _._ 9 _,_ 1) _,_ ( _a −_ 0 _._ 9 _,_ 1) _∈ Q_ (say (0 _._ 9 _,_ 1)). Then _S_ contains line segment [(0 _._ 9 _,_ 1) _,_ (1 _,_ 1)] and hence _σ_ ( _S_ ) _≥ σ_ ( _S_ ; _R_<sup>_∗_</sup> ) + 0 _._ 5 + 0 _._ 45 + 0 _._ 05 _>_ 1. 

We now consider the case where _S_ contains no point in _P_ . Then, by Lemma 7(iii), _S_ contains at least one of points (1 _,_ 0 _._ 9) _,_ ( _a −_ 1 _,_ 0 _._ 9) _∈ Q_ . Assume that _S_ contains (1 _,_ 0 _._ 9) (the case that _S_ contains ( _a −_ 1 _,_ 0 _._ 9) can be treated analogously). If _S_ contains point (0 _._ 9 _,_ 1), then it contains line segments [(0 _._ 9 _,_ 1) _,_ (1 _,_ 1)] and [(1 _,_ 0 _._ 9) _,_ (1 _,_ 1)], and _σ_ ( _S_ ) _≥ σ_ ( _S_ ; _R_<sup>_∗_</sup> ) + (0 _._ 45 + 0 _._ 1 _×_ 0 _._ 5) _×_ 2 _>_ 1 holds. Similarly if _S_ contains ( _a −_ 1 _,_ 0 _._ 9), then we can show that _S_ contains line segments [( _a −_ 1 _,_ 0 _._ 9) _,_ ( _a −_ 1 _,_ 1)] and [( _a −_ 0 _._ 9 _,_ 1) _,_ ( _a −_ 1 _,_ 1)], implying _σ_ ( _S_ ) _≥_ 1. Then assume that _S_ contains none of points (0 _._ 9 _,_ 1) and ( _a −_ 1 _,_ 0 _._ 9). If _S_ contains point (min _{_ 2 _, a −_ 1 _},_ 0 _._ 9) then _σ_ ( _S_ ) _≥_ 0 _._ 45 _×_ 2 + _d_ + 0 _._ 5 _c >_ 1 by Lemma 5. Assume further that _S_ does not contain point (min _{_ 2 _, a −_ 1 _},_ 0 _._ 9); _a ≥_ 3 is assumed (the case of _a <_ 3 can be treated analogously). To estimate the minimum _σ_ ( _S_ ) in this case, we can assume that one corner of _S_ touches the _x_ -axis and point (2 _,_ 0 _._ 9) is on an edge of _S_ . If point (1 _,_ 1) is in _S_ , then _σ_ ( _S_ ; _L_ 3) + _σ_ ( _S_ ; _Q_ ) _≥_ 0 _._ 5 and _σ_ ( _S_ ) _≥ d_ + 0 _._ 5 _c_ + 0 _._ 5 _>_ 1 by Lemma 5. Assume that (1 _,_ 1) is not in _S_ . Let _p_<sup>_′_</sup> = (1 _,_ 1 _− c_<sup>_′_</sup> ) be the crossing point of line segment [(1 _,_ 0 _._ 9) _,_ (1 _,_ 1)] and an edge of _S_ , where line segment [ _p_<sup>_′_</sup> _,_ (1 _,_ 1)] is not covered by _S_ . Note that _σ_ ( _S_ ; _L_ 1) = 0 _._ 5 _c_ and _σ_ ( _S_ ; _L_ 3) + _σ_ ( _S_ ; _Q_ ) = 0 _._ 5 _−_ 0 _._ 5 _c_<sup>_′_</sup> . Then _σ_ ( _S_ ) _≥ d_ + 0 _._ 5 _c_ + 0 _._ 5 _−_ 0 _._ 5 _c_<sup>_′_</sup> , which is greater than 1 by Lemma 6. 

### **5.6 Case-7** 

Finally we consider the case where the center of _S_ belongs to the rectangle [1 _, a−_ 1] _×_ [0 _,_ 1], and line _y_ = 1 intersects two nonadjacent edges _e_ 1 and _e_ 2 of _S_ . Let _L_<sup>_′_</sup> be the line segment 

> the electronic journal of combinatorics **12** (2005), #R37 

2 

11 

obtained as the intersection of line _y_ = 1 and _S_ , and _c_ be the length of _L_<sup>_′_</sup> . Then _c ≥ λ >_ 1 since _y_ = 1 intersects two nonadjacent edges of _S_ . If _L_<sup>_′_</sup> is contained in _L_ 1 (i.e., _S_ contains none of (0 _._ 9 _,_ 1) _,_ ( _a −_ 0 _._ 9 _,_ 1) _∈ Q_ ) and _S_ contains a point in _P_ , then _σ_ ( _S_ ) _≥_ 0 _._ 5 _c_ + 0 _._ 5 _>_ 1. 

We next consider the case where _S_ contains one of (0 _._ 9 _,_ 1) _,_ ( _a −_ 0 _._ 9 _,_ 1) _∈ Q_ . If _S_ contains both (0 _._ 9 _,_ 1) and ( _a −_ 0 _._ 9 _,_ 1), then _L_ 1 is entirely contained in _S_ , implying _σ_ ( _S_ ) _≥ σ_ ( _S_ ; _R_<sup>_∗_</sup> ) + (0 _._ 45 + 0 _._ 05) _×_ 2 _>_ 1. Assume that _S_ contains (0 _._ 9 _,_ 1) but not ( _a −_ 0 _._ 9 _,_ 1) (the other case can be treated analogously). By Lemma 7(iii), _S_ contains (2 _,_ 0 _._ 9) or (1,0.9). In any case, _S_ contains line segment [(0 _._ 9 _,_ 1) _,_ (1 _,_ 1)] and point (2 _,_ 0 _._ 9) (or line segment [(1 _,_ 0 _._ 9) _,_ (1 _,_ 1)]), indicating _σ_ ( _S_ ) _≥ σ_ ( _S_ ; _R_<sup>_∗_</sup> ) + 0 _._ 5 + 0 _._ 5 _>_ 1. 

We finally consider the case where _L_<sup>_′_</sup> is contained in _L_ 1 and _S_ contains no point in _P_ . Then by Lemma 7(iii) _S_ contains (1 _,_ 0 _._ 9) or ( _a −_ 1 _,_ 0 _._ 9); We assume that (1 _,_ 0 _._ 9) is in _S_ (the other case can be treated analogously). If _S_ contains (1 _,_ 1), then it also contains line segment [(1 _,_ 0 _._ 9) _,_ (1 _,_ 1)] and satisfies _σ_ ( _S_ ) _≥_ 0 _._ 5 _c_ + 0 _._ 5 _>_ 1. Hence the remaining case is that _S_ contains (1 _,_ 0 _._ 9) but none of (1 _,_ 1) and (min _{_ 2 _, a −_ 1 _},_ 0 _._ 9) (see Fig. 6). To estimate the minimum _σ_ ( _S_ ) in this case, we can assume that _S_ touches the _x_ -axis (allowing it to violate the condition that _y_ = 1 intersects two nonadjacent edges of _S_ ). Now _y_ = 1 intersects two adjacent edges of _S_ and this case has already been discussed in Case-6. 



<!-- Start of picture text -->
y= 1<br>S<br><!-- End of picture text -->

Figure 6: Illustration for the case where _S_ contains (1 _,_ 0 _._ 9) but none of (1 _,_ 1) and (min _{_ 2 _, a −_ 1 _},_ 0 _._ 9) in Case-7. 

This completes the proof of Lemma 1. 

## **6 Concluding Remarks** 

In this paper, we have established a nontrivial upper bound on the number of unit squares that can be packed into a rectangle with given side lengths. With this bound, we have derived a stronger lower bound on _s_ ( _N_ ) and determined that _s_ ( _n_<sup>2</sup> _−_ 1) = _s_ ( _n_<sup>2</sup> _−_ 2) = _n_ for all integers _n ≥_ 2. Our unavoidable set _U_ can be seen as a modification of the entire area _R_ so that the total score becomes less than the area of _R_ by replacing the boundary part of _R_ with a set of points and line segments with appropriate scores. This technique 

12 

the electronic journal of combinatorics **12** (2005), #R37 

can be easily applied to the problem of packing unit squares into other types of convex polygons such as regular _k_ -gons ( _k ≥_ 3) by modifying the definition of endpoints in _Q_ and their scores. 

## **Acknowledgment** 

We would like to express our gratitude to the anonymous referees whose suggestions contributed to improving the written style. This research was partially supported by a Scientific Grant in Aid from the Ministry of Education, Culture, Sports, Science and Technology of Japan. 

## **References** 

- [1] H. T. Croft, K. J. Falconer, and R. K. Guy, Unsolved Problems in Geometry, Springer Verlag, Berlin (1991) 108–114. 

- [2] P. Erd˝os and R. L. Graham, On packing squares with equal squares, J. Combin. Theory Ser. A, 19 (1975) 119–123. 

- [3] E. Friedman, Packing unit squares in squares: A survey and new results, The Electronic Journal of Combinatorics, Dynamic Surveys (#DS7), (2000). 

- [4] F. G¨obel, Geometrical packing and covering problems, in Packing and Covering in Combinatorics, A. Schrijver (ed.), Math Centrum Tracts, 106 (1979) 179–199. 

- [5] M. J. Kearney and P. Shiu, Efficient packing of unit squares in a square, The Electronic Journal of Combinatorics, 9 (2002) (#R14). 

13 

the electronic journal of combinatorics **12** (2005), #R37 

