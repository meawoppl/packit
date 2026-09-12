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

## Licenses of included papers

The papers in `downloads/` (PDF) and `markdown/` (text conversions) are **not**
covered by this repository's Apache-2.0 license. Each keeps its own copyright
and terms, and they differ:

| File | Copyright / license |
| --- | --- |
| `stromquist-2003` | © the author. Published by the Electronic Journal of Combinatorics under a non-exclusive publication agreement; no open license attached. |
| `nagamochi-2005` | © the author. EJC publication agreement; no open license attached. |
| `bentz-13-46-2010` | © the author. EJC publication agreement; no open license attached. |
| `bentz-22-33-2016` | © the author. [arXiv non-exclusive distribution license 1.0](http://arxiv.org/licenses/nonexclusive-distrib/1.0/), which grants rights to arXiv only. |
| `wang-dong-li-2016` | © the authors. arXiv non-exclusive distribution license 1.0. |
| `abrahamsen-stade-2024` | © the authors. [CC BY 4.0](https://creativecommons.org/licenses/by/4.0/); reuse with attribution. |

EJC terms: https://www.combinatorics.org/ojs/index.php/eljc/about/submissions
(papers before March 2018 generally carry no explicit license). They are kept
here as research references with full attribution; follow the original links
above for the canonical versions.

## Typeset transcriptions

The six papers in [`markdown/`](markdown/) started as PDF extractions with
[pymupdf4llm](https://pypi.org/project/pymupdf4llm/). Their mathematics has been
restored against the committed PDFs, using GitHub inline math and fenced
`math` displays. Each file keeps its source and license header. Historical
claims remain as printed; separate transcription notes identify apparent
source errors rather than silently changing the mathematics. Figure captions
link to the PDF diagrams where the extracted image text was unusable.

Do not overwrite a checked transcription with fresh extraction output. To
compare a newer PDF, extract into a scratch file, retain the source/license
header, and review changes against rendered PDF pages. The extraction command
emits only a draft body:

```sh
uv run --with pymupdf4llm python -c "import pymupdf4llm,sys; print(pymupdf4llm.to_markdown(sys.argv[1]))" downloads/<id>.pdf > /tmp/paper-draft.md
```

[The reference math workflow](../.github/workflows/math.yml) runs
[CosmicFrontierLabs/github-math-lint](https://github.com/CosmicFrontierLabs/github-math-lint)
on `refs/**/*.md` and pull-request text. It is pinned to a reviewed commit and
uploads the full report. This detects GitHub Markdown/math rendering hazards;
it does not verify a theorem or catch every incorrectly transcribed symbol.
Changes still need comparison with the source PDF and inspection of GitHub's
rendered Markdown, including fractions, radicals, subscripts and equation labels. Numbered
equations use adjacent prose labels: `\tag{...}` currently produces labeled
MathML rows that render incorrectly in GitHub viewed with Chrome. Keep math
outside link labels and footnotes, and use `^{\ast}` instead of a bare star
that Markdown can consume as emphasis. GitHub's dollar/backtick inline syntax
handles formulas next to ambiguous prose punctuation. In the longer papers,
standalone single-letter symbols can use italic prose to stay within GitHub's
per-document math rendering budget; compound formulas remain math.

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
