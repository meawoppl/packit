> **Source:** Walter Stromquist, *Packing 10 or 11 Unit Squares in a Square*, Electronic Journal of Combinatorics 10 (2003) #R8. https://doi.org/10.37236/1701
>
> **License:** Copyright held by the author(s). Published by the Electronic Journal of Combinatorics under a non-exclusive publication agreement; no open license was attached (pre-2018 EJC paper).
>
> Machine conversion of [`../downloads/stromquist-2003.pdf`](../downloads/stromquist-2003.pdf) with pymupdf4llm. Formulas, figures, and tables are often garbled; cite and check the PDF.

# Packing 10 or 11 Unit Squares in a Square 

Walter Stromquist 

Department of Mathematics Bryn Mawr College, Bryn Mawr, Pennsylvania, USA 

```
walters@chesco.com
```

Submitted: Nov 26, 2002; Accepted: Feb 26, 2003; Published: Mar 18, 2003 MR Subject Classifications: 05B40, 52C15 

### **Abstract** 

Let _s_ ( _n_ ) be the side of the smallest square into which it is possible pack _n_ unit squares. We show that _s_ (10) = 3 + � 12<sup>_≈_3</sup><sup>_._707andthat</sup><sup>_s_(11)</sup><sup>_≥_2 + 2</sup> � 45<sup>_≈_3</sup><sup>_._789.</sup> We also show that an optimal packing of 11 unit squares with orientations limited to 0<sup>_◦_</sup> or 45<sup>_◦_</sup> has side 2+2� 89<sup>_≈_3</sup><sup>_._886.These results prove Martin Gardner’s conjecture</sup> that _n_ = 11 is the first case in which an optimal result requires a non-45<sup>_◦_</sup> packing. 

Let _s_ ( _n_ ) be the side of the smallest square into which it is possible to pack _n_ unit squares. It is known that _s_ (1) = 1, _s_ (2) = _s_ (3) = _s_ (4) = 2, _s_ (5) = 2 + � 12<sup>,andthat</sup> _s_ (6) = _s_ (7) = _s_ (8) = _s_ (9) = 3 _._ For larger _n_ , proofs of exact values of _s_ ( _n_ ) have been published only for _n_ = 14, 15, 24, 35, and when _n_ is a square. The first published proof that _s_ (6) = 3 is by Kearney and Shiu [3] and the other results are reported in Erich Friedman’s dynamic survey [1]. 

We prove here that _s_ (10) = 3 + � 12<sup>_≈_3</sup><sup>_._707 (Theorem1)and that</sup><sup>_s_(11)</sup><sup>_≥_2 + 2</sup> � 45<sup>_≈_</sup> 3 _._ 789 (Theorem 2). The 10-square packings in Figure 1 are optimal. The most efficient known packing of 11 squares, shown in Figure 2 and due to Walter Trump, has side about 3 _._ 8772 and includes unit squares tilted at about 40 _._ 182<sup>_◦_</sup> . 



<!-- Start of picture text -->
s  = 3 + � 12 ≈ 3 . 707<br><!-- End of picture text -->









Figure 1: Best packings of 10 squares 

1 

the electronic journal of combinatorics **10** (2003), #R8 



<!-- Start of picture text -->
s ≈ 3 . 8772<br><!-- End of picture text -->





<!-- Start of picture text -->
s ≈ 3 . 886<br><!-- End of picture text -->



Figure 2: Best known packing of 11 squares (tilt _≈_ 40 _._ 182<sup>_◦_</sup> ) 

Figure 3: Optimal 45<sup>_◦_</sup> packing for _n_ = 11 

In the case of _n_ = 11, we also show that any 45<sup>_◦_</sup> packing—that is, one in which the unit squares are tilted only at 0<sup>_◦_</sup> or 45<sup>_◦_</sup> with respect to the bounding square—must have side at least 2 + 2� 89<sup>_≈_3</sup><sup>_._886(Theorem3).Thisboundisrealizedbythepackingby</sup> H¨am¨al¨ainen [2] in Figure 3. Together, these results establish the truth of Martin Gardner’s conjecture in [7], that _n_ = 11 is the first case in which non-45<sup>_◦_</sup> packings are required. 

These results were first reported in [4,5,6]. We take the approach that was used in those memoranda and also used in [1] for establishing lower bounds. For rhetorical reasons, we define a _box_ to be the interior of any square of side strictly greater than 1. In order to establish a lower bound of the form _s_ ( _n_ ) _≥ a_ , we prove the equivalent statement that _n_ nonoverlapping boxes cannot be packed inside a square with side exactly _a_ . For the most part we treat boxes as if they were unit squares, and rely on the extra margin of size to convert equations into inequalities as needed. 

## **1 Nonavoidance Lemmas** 

In this section we present six “nonavoidance lemmas.” Each lemma provides that if the center of a box is in some region, then the box must have a nonempty intersection with certain parts of the region’s boundary. Lemmas 1–4 are general in nature, while Lemmas 5 and 6 are needed specifically for the proofs of Theorems 1 and 2 respectively. The lemmas are illustrated in Figure 4. 

The first three lemmas are the same as Lemmas 1–3 in [1]. 

**Lemma 1** _Let a ≤_ 1 _and b ≤_ 1 _. Then any box whose center is in the rectangle_ [0 _, a_ ] _×_ [0 _, b_ ] _must intersect the x-axis, the y-axis, or the point_ ( _a, b_ ) _._ 

**Lemma 2** _Let T be a triangle with sides of length at most_ 1 _. Then any box whose center is in the interior of T must contain one of the vertices of T ._ 

2 

the electronic journal of combinatorics **10** (2003), #R8 



<!-- Start of picture text -->
≤1 ≤<br>≤1 ≤1<br>≤1 ≤1<br>≤1<br>�1�1� � 1�<br>�1<br>� 1<br>�1<br>1<br>≤ 1<br>≤<br><!-- End of picture text -->

Figure 4: The nonavoidance lemmas 

**Lemma 3** _Let a and b satisfy a ≤_ 1 _, b ≤_ 1 _, and a_ + 2 _b ≤_ 2 _√_ 2 _. Then any box whose center is in the rectangle_ [0 _, a_ ] _×_ [0 _, b_ ] _must intersect the x-axis, the point_ (0 _, b_ ) _, or the point_ ( _a, b_ ) _._ 

We use Lemma 3 mainly in the case of _a_ = 2 _√_ 2 _−_ 2 _≈ ._ 828, _b_ = 1, as shown in Figure 4. The other extreme is _a_ = 1, _b_ = _√_ 2 _−_<sup>1</sup> 2<sup>_≈._914.</sup> 

We need some preparation for Lemma 4. When 2 _√_ 2 _−_ 2 _< a <_ 1, define _f_ ( _a_ ) by 



where _θ_<sup>_∗_</sup> is the smallest positive value of _θ_ that satisfies 



For values of _a_ in the domain of _f_ we always have 0 _< θ_<sup>_∗_</sup> _<_ 45<sup>_◦_</sup> and 0 _< f_ ( _a_ ) _<_ 1. 

**Lemma 4** _Let a and b satisfy_ 2 _√_ 2 _−_ 2 _< a <_ 1 _,_ 0 _< b <_ 1 _,_ ( _a, b_ ) _within_ 1 _of_ (0 _,_ 1) _, and b ≤ f_ ( _a_ ) _. Then any box whose center is in the quadrilateral with vertices_ (0 _,_ 0) _,_ (0 _,_ 1) _,_ ( _a,_ 0) _, and_ ( _a, b_ ) _must intersect the x-axis, the point_ (0 _,_ 1) _, or the point_ ( _a, b_ ) _._ 

We rely on these cases of Lemma 4: 



3 

the electronic journal of combinatorics **10** (2003), #R8 



<!-- Start of picture text -->
���1�<br>1-a cos θ<br>� sin θ<br>z�cos θ<br>�a����<br>z 1-a cos θ<br>�<br>θ<br>a cos  θ<br><!-- End of picture text -->

Figure 5: Proof of Lemma 4 

**Proof.** If a box avoids both the _x_ -axis and the point (0 _,_ 1), then its edge might as well touch both as shown in Figure 5. Let ( _a, b_<sup>_∗_</sup> ) be the point at which the box’s top edge meets the line _x_ = _a_ . The two triangles marked * are congruent. Since _z_ + _z/_ cos _θ_ = 1, we have _z_ = 1+coscos _θ θ_<sup>and</sup> 



If we fix _a_ in the range 2 _√_ 2 _−_ 2 _< a <_ 1 and limit _θ_ to the first quadrant, then the right side of (3) has a unique minimum, which occurs when _θ <_ 45<sup>_◦_</sup> and _b_<sup>_∗_</sup> _<_ 1. (To verify this, note that _b_<sup>_∗_</sup> is large when _θ ≈_ 0, below 1 when _θ_ = 45<sup>_◦_</sup> , and decreasing to 1 when _θ_ = 90<sup>_◦_</sup> , and that the derivative doesn’t have enough roots for there to be multiple minima below 45<sup>_◦_</sup> .) Setting _db_<sup>_∗_</sup> _/dθ_ = 0, leads to equation (2) above. Therefore _f_ ( _a_ ) is the minimum value of _b_<sup>_∗_</sup> . 

If ( _a, b_ ) is within 1 of (0 _,_ 1) but below ( _a, f_ ( _a_ ))—and hence below ( _a, b_<sup>_∗_</sup> ) whatever the value of _θ_ —then ( _a, b_ ) is clearly inside the box. _2_ 

**Lemma 5** _Let P be the pentagon with vertices_ (1 _,_ 0) _,_ (1 _,_ 1) _,_ (2 _,_ 1) _,_ (2 _._ 12 _, ._ 9) _, and_ (2 _._ 12 _,_ 0) _. Then any box whose center is in the interior of P must intersect the x-axis, the segment from_ (1 _,_ 0 _._ 788) _to_ (1 _,_ 1) _, or the segment form_ (2 _,_ 1) _to_ (2 _._ 12 _, ._ 90) _._ 

**Proof.** By Lemma 1, any counterexample must include a point to the left of _x_ = 1 and a point to the right of _x_ = 2. Without loss of generality, the box’s boundary touches the _x_ axis and includes the point (1 _, ._ 788) as shown in Figure 6. Let _θ_ be the angle of the box with the _x_ axis, as shown. 

If _θ ≤_ tan<sup>_−_1</sup> (1 _._ 2) _≈_ 50 _._ 2<sup>_◦_</sup> , then the box must contain the point (2 _,_ 1). To see this, we calculate the _x_ -coordinate of the point ( _x,_ 1) at which the box’s upper-right boundary crosses the line _y_ = 1: 



the electronic journal of combinatorics **10** (2003), #R8 





<!-- Start of picture text -->
(2,1)<br>(1,1)<br>(2.12,.9)<br>(1,.788)<br>θ<br><!-- End of picture text -->

Figure 6: Proof of Lemma 5 

The last term is the length of the box’s intersection with the line _y_ = 1, and it exceeds _._ 828 for any first-quadrant value of _θ_ , so when tan _θ ≤_ 1 _._ 2 we have 



forcing the point (2 _,_ 1) to be inside the box. 

If tan<sup>_−_1</sup> (1 _._ 2) _< θ ≤_ sin<sup>_−_1</sup> ( _._ 9) _≈_ 64 _._ 2<sup>_◦_</sup> , then the box contains the point (2 _._ 12 _, ._ 9). To see this, we compute the _x_ -coordinate of the point at which the box’s upper-right boundary intersects the line _y_ = _._ 9. We obtain 



This function reaches its minimum at _θ ≈_ 52 _._ 6<sup>_◦_</sup> , when _x_ = 2 _._ 1256, so the box’s right boundary always passes to the right of (2 _._ 12 _, ._ 9). 

If sin<sup>_−_1</sup> ( _._ 9) _< θ_ we need to calculate the coordinates ( _x, y_ ) of the box’s rightmost vertex: 



Since _._ 9 _< y <_ 1, the vertex is to the right of the critical segment if ( _x −_ 2) _/_ (1 _− y_ ) _>_ 1 _._ 2. Some calculation shows that 



When sin _θ > ._ 9 the first term on the right is at least _._ 6 and the second term is at least 1, so the lemma is proved. _2_ 

5 

the electronic journal of combinatorics **10** (2003), #R8 

**Lemma 6** _Let a_ = � 45<sup>_≈._894</sup><sup>_._</sup> _Then any box whose center is in the pentagon with vertices at_ (1 _,_ 0) _,_ (1 _,_ 1) _,_ (1 +<sup>1</sup> 2<sup>_a,_1</sup><sup>_._12)</sup><sup>_,_(1 +</sup><sup>_a,_1)</sup><sup>_,and_(1 +</sup><sup>_a,_0)</sup><sup>_mustintersectthex-axis_</sup> _or one of the vertices._ 

**Proof.** We may assume that any counterexample involves a box touching the _x_ -axis as in Figure 7. If _D_ ( _θ_ ) is the length of the intersection of the box with the line _y_ = 1, then 



Let _θ_ 0 = 12<sup>sin</sup><sup>_−_1 �</sup> 5 _−_ 2 _√_ 5� _≈_ 15 _._ 9<sup>_◦_</sup> ; then _D_ ( _θ_ 0) = _a_ . If _θ < θ_ 0 or _θ > π_ 2<sup>_−θ_0then</sup> _D_ ( _θ_ ) _> a_ and the box must intersect (1 _,_ 1) or (1 + _a,_ 1). We can therefore assume that _θ_ 0 _≤ θ ≤_<sup>_π_</sup> 2<sup>_−θ_0.Inthiscasecos</sup><sup>_θ_+ sin</sup><sup>_θ>_1</sup><sup>_._12,sotheboxincludesapointabove</sup> _y_ = 1 _._ 12. We may assume that the box touches the point (1 + _a,_ 1) and has its apex to the right of the line _x_ = 1 +<sup>1</sup> 2<sup>_a_,asshowninthefigure.Nowthe</sup><sup>_y_coordinateatwhich</sup> the top of the box intersects the line _x_ = 1 +<sup>1</sup> 2<sup>_a_isgivenby</sup> 



which by direct computation is equal to 1 _._ 1277 _..._ when _θ_ = _θ_ 0, and increases with _θ_ . Therefore the box intersects the line above the point (1+<sup>1</sup> 2<sup>_a,_1</sup><sup>_._12), and mustinclude that</sup> point. _2_ 



<!-- Start of picture text -->
(1+a��,1�1�)<br>(1,1) (1+a,1)<br>θ<br><!-- End of picture text -->

Figure 7: Proof of Lemma 6 

6 

the electronic journal of combinatorics **10** (2003), #R8 

## **2 Ten Squares** 

**Theorem 1** _Ten pairwise nonintersecting boxes cannot exist in the interior of a square of side s_ = 3 + � 12<sup>_._</sup> 

**Proof.** In this section, fix _s_ = 3 + � 12<sup>andlet</sup><sup>_S_bethesquare[0</sup><sup>_, s_]2.Definetenpoints</sup> _A, B, . . . , J_ as shown in Figure 8. We set _A_ = (1 _,_ 1), _B_ = ( _._ 97 _,_ 2<sup>_s_),</sup><sup>_I_= (1</sup><sup>_._4</sup><sup>_,_</sup> 2<sup>_s_),andplace</sup> the other points symmetrically in _S_ . Each of the regions outlined in the figure is covered by one of Lemmas 1, 2, or 4 (with _a ≈ ._ 853, _b_ = _._ 97). It follows that these ten points are _unavoidable_ in the sense of [1], meaning that any box inside _S_ must contain one of the points. If ten boxes are packed in _S_ , each must contain exactly one of them. We name the boxes for the points they contain—A-box, B-box, etc. 



<!-- Start of picture text -->
C D E<br>I J<br>F<br>B<br>A H G<br><!-- End of picture text -->

Figure 8: Each box contains one of these ten points 

The key to the proof is to show that the H-box also contains some point on the short segment from (2 _,_ 1) to (2 _._ 12 _, ._ 9). We will prove this fact and then show why it matters. 

1. The points remain unavoidable if _B_ is replaced by _B_<sup>_′_</sup> = ( _._ 75 _, s −_ 1 _._ 96). Therefore, the point _B_<sup>_′_</sup> is contained in the B-box. (We now use Lemma 4 with _a_ = _._ 96, _b_ = _._ 75.) 



<!-- Start of picture text -->
C D E<br>I J<br>F<br>B'<br>A''<br>H G<br>A'<br><!-- End of picture text -->



<!-- Start of picture text -->
C D E<br>I J<br>F<br>B'<br>A''<br>H G<br>A'<br><!-- End of picture text -->

Figure 9: A-box contains one of A<sup>_′_</sup> , A<sup>_′′_</sup> 

Figure 10: If A-box contains A<sup>_′′_</sup> , then H-box contains H<sup>_′_</sup> =(2,1) 

7 

the electronic journal of combinatorics **10** (2003), #R8 



<!-- Start of picture text -->
* *<br>*<br>B *<br>* *<br>* *<br><!-- End of picture text -->

Figure 11: H-box must touch segment 

Figure 12: No room for I-box and J-box 

2. If, now, _A_ is replaced by the two points _A_<sup>_′_</sup> = (1 _, s −_ 2 _._ 92) and _A_<sup>_′′_</sup> = (1 _._ 2 _,_ 1), the points remain unavoidable (Figure 9). It follows that the A-box must contain at least one of the points _A_<sup>_′_</sup> , _A_<sup>_′′_</sup> . Note that _s −_ 2 _._ 92 _< ._ 788. 

3. If the A-box contains _A_<sup>_′′_</sup> = (1 _._ 2 _,_ 1), then the points _A_ , _A_<sup>_′′_</sup> , _B_<sup>_′_</sup> , _C_ through _G_ , _I_ , _J_ , and (2 _,_ 1) form an unavoidable set (Figure 10). All of these are denied to the H-box except for (2 _,_ 1), so the H-box contains (2 _,_ 1). (This step uses Lemma 3.) 

4. If the A-box contains _A_<sup>_′_</sup> = (1 _, s−_ 2 _._ 92), then the entire segment from _A_<sup>_′_</sup> to _A_ (which includes the segment from (1 _, ._ 788) to (1 _,_ 1)) is denied to the H-box, as are points _B_ through _G_ , _I_ , and _J_ . Figure 11 shows a partition of _S_ in which Lemma 5 applies to one of the regions. From this figure, we see that the H-box must touch the segment from (2 _,_ 1) to (2 _._ 12 _, ._ 9). 

In either case, the H-box must contain some point on the indicated segment. In Figure 12 the point of intersection is marked with an asterisk. Seven other asterisks mark other points which must be contained in the B-, D-, F-, and H-boxes by symmetrical arguments. We do not know the locations of these points exactly, but we can tell that each asterisk is within 1 of the center of the square and within 1 of each of the two asterisks nearest to it. Each of the heavy line segments connects two asterisks that must be in the same box. 

Now, the thirteen points in Figure 12—the eight asterisks, the points _A, C, E, G_ , and the center of the square—clearly form an unavoidable set. All but the center are denied to the I- and J-boxes, and those two boxes cannot both contain the center. This shows that the 10-box packing is impossible. _2_ 

8 

the electronic journal of combinatorics **10** (2003), #R8 





<!-- Start of picture text -->
E<br>F D<br>H I<br>G<br>C<br>J<br>B<br>A<br><!-- End of picture text -->

Figure 13: Ten points to avoid and how to avoid them 

Figure 14: Twelve points for Theorem 2 

## **3 Eleven Squares** 

**Theorem 2** _Let s_ = 2 + 2� 45<sup>_≈_3</sup><sup>_._789</sup><sup>_.Thenelevennon-intersectingboxescannotexist_</sup> _inside a square of side s._ 

**Proof.** For this proof, fix _s_ = 2 + 2� 45<sup>andlet</sup><sup>_S_=[0</sup><sup>_, s_]2.Considerthetenpointsin</sup> Figure 13. Four of these points have coordinates (1 _,_ 1), ( 2<sup>_s,_1),(3</sup> 2<sup>_−_</sup> 4<sup>_s,_</sup> 2<sup>_s_),( 1</sup> 2<sup>+</sup> 4<sup>_s,_</sup> 2<sup>_s_),and</sup> the rest are placed symmetrically. The vertical distance between the rows of points is 2 _s_<sup>_−_1=</sup> � 45<sup>_≈._894.Thetrianglesinthefigureareallcongruent,andtheslopingsides</sup> have length 1. 

Nonavoidance lemmas apply to all of the regions shown except for the rectangles at the top and bottom. If 11 boxes are to be packed into the square, at least one of them must be placed in one of those rectangles, roughly as shown in the figure (up to symmetry). From Lemmas 4 and 6 we can see that this box must contain all three of the points marked “A” in Figure 14: 



There are nine other points in Figure 14: 



9 

the electronic journal of combinatorics **10** (2003), #R8 

_I_ = (2 _._ 1 _,_ 2 _._ 1) _J_ = (2 _._ 1 _,_ 1 _._ 5) 

Nonavoidance lemmas apply to all of the regions in this figure. Since three of the twelve points are in one box, there cannot be eleven nonintersecting boxes. This completes the proof of Theorem 2. _2_ 

The argument in Figure 14 is not rigid; any point in the figure could be moved by a small amount in almost any direction without causing the argument to fail. The critical distances are all in Figure 13. 

**45**<sup>_◦_</sup> **packings.** We now apply the same technique to the case of 45<sup>_◦_</sup> packings. By considering only boxes that are oriented at 0<sup>_◦_</sup> or 45<sup>_◦_</sup> to the axes (“0<sup>_◦_</sup> and 45<sup>_◦_</sup> boxes”) we can prove stronger forms of some of our lemmas. In particular: 

**Lemma 7** _Let T be a triangle, and suppose that the component of any side of T in the direction of any unit vector making an angle of_ 0<sup>_◦_</sup> _or_ 45<sup>_◦_</sup> _with either axis is at most_ 1 _. Then any_ 0<sup>_◦_</sup> _or_ 45<sup>_◦_</sup> _box whose center is in the interior of T must contain one of the vertices of T ._ 

**Lemma 8** _If_ ( _a, b_ ) = (1 _, ._ 8) _or_ ( _a, b_ ) = (<sup>2</sup> 3 _√_ 2 _,_ 2 _√_ 2 _−_ 2) _, then any_ 0<sup>_◦_</sup> _or_ 45<sup>_◦_</sup> _box whose center is in the quadrilateral with vertices_ (0 _,_ 0) _,_ (0 _,_ 1) _,_ ( _a,_ 0) _, and_ ( _a, b_ ) _must intersect the x-axis, the point_ (0 _,_ 1) _, or the point_ ( _a, b_ ) _._ 

The proofs are easier than in the general case and are omitted. With these more powerful lemmas, we can justify a larger value of _s_ in the following theorem, which is enough to settle Martin Gardner’s conjecture. 

**Theorem 3** _Let s_ = 2 +<sup>4</sup> 3 _√_ 2 _≈_ 3 _._ 886 _. Then eleven non-intersecting boxes cannot exist inside a square of side s, if each box has orientation_ 0<sup>_◦_</sup> _or_ 45<sup>_◦_</sup> _with respect to the square._ 

**Proof.** Now fix _s_ = 2+<sup>4</sup> 3 _√_ 2 and let _S_ = [0 _, s_ ]<sup>2</sup> . Consider ten points defined exactly as in Figure 13—four of these points have coordinates (1 _,_ 1), ( 2<sup>_s,_1), (3</sup> 2<sup>_−_</sup> 4<sup>_s,_</sup> 2<sup>_s_), ( 1</sup> 2<sup>+</sup> 4<sup>_s,_</sup> 2<sup>_s_)—but</sup> with the new value of _s_ . If eleven boxes are to be packed into the square, one of them will have to avoid the marked points. This is impossible for a box with 0<sup>_◦_</sup> orientation. The interior sloping lines now have length � 109<sup>,buttheircomponentsin thedirection</sup> of a 45<sup>_◦_</sup> unit vector are at most 1, so Lemma 7 applies to the triangles in the figure. It follows that a 45<sup>_◦_</sup> box that avoids the points must be (up to symmetry) in approximately the position shown in Figure 13. This box must contain three points like those marked “A” in Figure 14, but now they have these coordinates: 



10 

the electronic journal of combinatorics **10** (2003), #R8 

The other nine points in Figure 14 become 



Again these 12 points form an unavoidable set in the context of 45<sup>_◦_</sup> packings, and since three of them are in one box, there cannot be 11 nonintersecting boxes. This completes the proof of Theorem 3 and establishes the truth of Martin Gardner’s conjecture. _2_ 

## **References** 

1. Erich Friedman, “Packing Unit Squares in Squares: A Survey and New Results,” _The Electronic Journal of Combinatorics_ **7** (2002), Dynamic Survey DS#7. 

2. Pertti H¨am¨al¨ainen, correspondence, April 20, 1980. 

3. Michael J. Kearney and Peter Shiu, “Efficient packing of unit squares in a square,” _The Electronic Journal of Combinatorics_ **9** (2002), #R14. 

4. Walter Stromquist, “Packing unit squares inside squares, I (six unit squares),” Daniel H. Wagner, Associates Memorandum, September 11, 1984 

5. ——, “Packing unit squares inside squares, II (ten unit squares),” DHWA Memorandum, October 15, 1984. 

6. ——, “Packing unit squares inside squares, III (Cases with _n ≤_ 65 and Martin Gardner’s conjecture for _n_ = 11),” DHWA Memorandum, November 15, 1984. 

7. Martin Gardner, “Mathematical Games” in _Scientific American_ , October 1979. (See also November 1979, March 1980, and November 1980.) 

11 

the electronic journal of combinatorics **10** (2003), #R8 

