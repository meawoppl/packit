# Square packing reference library

Checked 2026-09-12. The game uses congruent **unit** squares, with arbitrary
rotation, in an axis-aligned square of side `s(n)`. Smaller side wins.

## Records and proof status

[`best_known.json`](best_known.json) is the API's record snapshot for n=1..100.
`side` is a published construction (an **upper bound**); `proven_optimal`
means a matching lower bound is known. Missing nontrivial constructions use
the integer grid construction, with proof status kept separate. Numerical
values are rounded references, not exact certificates or rejection thresholds.

The current construction source is [David Ellsworth's Squares in Squares](https://kingbird.myphotos.cc/packing/squares_in_squares.html),
the maintained successor to [Erich Friedman's page](https://erich-friedman.github.io/packing/squinsqu/).
It includes high-precision SVG geometry and polynomial forms, with records
updated during 2025–2026. The snapshot records the displayed values; pending
improvements or incompletely optimized constructions remain upper bounds.

Useful benchmarks: s(5)=2+1/sqrt(2), s(10)=3+1/sqrt(2). The 11-square
construction has side approximately 3.87708359002281, while Stromquist's
lower bound is 2+4/sqrt(5). Thus a gap to the construction and a gap to a
proven lower bound answer different questions. The universal area lower bound
sqrt(n) is available for every n, but often weak.

## Primary literature

The bibliography below follows the [Wikipedia starting page](https://en.wikipedia.org/wiki/Square_packing)
then checks the original publications. Downloadable author/publisher copies
are indexed by `sources.json`. PDFs are committed in `downloads/`; `fetch.py`
re-downloads available copies, checks their signatures, and records SHA-256
hashes in `downloads/manifest.json`.
Third-party papers retain their own copyrights and licenses; the repository's
software license does not relicense them.

| Work | Relevance | Primary link |
| --- | --- | --- |
| Erich Friedman, *Packing Unit Squares in Squares: A Survey and New Results*, DS7, 1998, revised 2009 | Historical constructions, lower bounds, proofs for small cases | [EJC](https://doi.org/10.37236/28) |
| Walter Stromquist, *Packing 10 or 11 Unit Squares in a Square*, 2003 | Exact result for 10, lower bound for 11 | [EJC](https://doi.org/10.37236/1701) |
| Hiroshi Nagamochi, *Packing Unit Squares in a Rectangle*, 2005 | Unavoidable-point lower bounds, including near-square counts | [EJC](https://doi.org/10.37236/1934) |
| Wolfram Bentz, *Optimal Packings of 13 and 46 Unit Squares in a Square*, 2010 | Proofs that s(13)=4 and s(46)=7 | [EJC](https://doi.org/10.37236/398) |
| Wolfram Bentz, *Optimal Packings of 22 and 33 Unit Squares in a Square*, 2016 preprint | Proofs that s(22)=5 and s(33)=6 | [arXiv](https://arxiv.org/abs/1606.03746) |
| Thierry Gensane and Philippe Ryckelynck, *Improved Dense Packings of Congruent Squares in a Square*, 2005 | Computational construction and refinement | [DCG](https://doi.org/10.1007/s00454-004-1129-z) |
| Paul Erdős and Ronald Graham, *On Packing Squares with Equal Squares*, 1975 | Sublinear wasted-area construction | [JCTA](https://doi.org/10.1016/0097-3165(75)90099-0) |
| Klaus Roth and Robert Vaughan, *Inefficiency in Packing Squares with Unit Squares*, 1978 | Asymptotic lower bound for wasted area | [JCTA](https://doi.org/10.1016/0097-3165(78)90005-5) |
| Fan Chung and Ronald Graham, *Packing Equal Squares into a Large Square*, 2009 | Improved asymptotic construction | [JCTA](https://doi.org/10.1016/j.jcta.2009.02.005) |
| Shuang Wang, Tian Dong, Jiamin Li, *A New Result on Packing Unit Squares into a Large Square*, 2016 | Further asymptotic construction | [arXiv](https://arxiv.org/abs/1603.02368) |
| Rory McClenagan, *Asymptotic Square Packing Problems*, MSc thesis, 2024 | Recent asymptotic packing research | [UNBC](https://doi.org/10.24124/2024/59553) |
| Peter Brass, William Moser, János Pach, *Research Problems in Discrete Geometry*, 2005, p.45 | Open-problem context | [Springer](https://doi.org/10.1007/0-387-29929-7) |
| Mikkel Abrahamsen and Jack Stade, *Hardness of Packing, Covering and Partitioning Simple Polygons with Unit Squares*, 2024 | Related computational complexity; not a bound for the square-container game | [arXiv](https://arxiv.org/abs/2404.09835) |

**Citation correction:** Wikipedia attaches arXiv:1606.03746 to the 13/46
paper. The arXiv page actually identifies the separate **22/33** paper. Keep
these records distinct. Wikipedia's table also should not be used alone to
assign proven-optimal status to every integer-grid case.

## Solver interpretation

The runtime solver uses center coordinates and c=cos(theta), s=sin(theta).
Corners are linear in x,y,c,s. Wall contacts are linear; vertex/edge contacts
are quadratic, with c²+s²=1. A wall-rooted contact graph supplies an outside-in
ordering. Local polynomial residual projection recovers a numerical candidate
on the selected contact branch. Independent SAT and containment checks decide
whether that candidate is a feasible score.

A small residual neither proves that all inequalities hold nor establishes a
global minimum. The exported equations and numerical root provide a starting
point for exact elimination/interval certification. For up to 25 squares with rotations near multiples of pi/4, exact Gaussian
elimination over Q(sqrt(2)) recovers a rational/quadratic side polynomial.
These snapped orientations are listed as explicit assumptions. General-purpose
minimal-polynomial elimination for arbitrary rotations and rigorous root
certification are not yet implemented. Recognized simple side expressions are explicitly labeled
numerical candidates. Do not label a gameplay submission a mathematical record
without independent high-precision verification and comparison with updated
construction data.
