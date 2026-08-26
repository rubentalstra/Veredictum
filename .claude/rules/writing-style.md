# Prose style — no AI tells

> Ported verbatim in substance from FerroEHR's `.claude/rules/writing-style.md`
> at the Veredictum split (FerroEHR#2789); only the scope list names this
> repository's surfaces.

Applies to every piece of prose a human reads as text: the `README.md`, the
docs site once it exists, `CONTRIBUTING.md` and its siblings, issue and PR
bodies, release notes, upstream reports, forum and announcement drafts, and
doc comments where they carry prose. It does not rewrite the vendored specs,
which are never edited, and it does not loosen the technical rules: citations,
honesty, and the comment budgets all still apply.

## The banned tells

1. **The "Not X, but Y" setup.** Framing points as contrasts: "It's not
   just a tool; it's an ecosystem", "X is not a feature, it is a
   philosophy", and the same move spelled "rather than", "instead of
   merely", "never simply". State what the thing IS and stop. A contrast is
   allowed when the reader genuinely holds the wrong belief and the sentence
   corrects it with facts on both sides.
2. **The rule of three.** Adjectives or clauses grouped in neat triads on a
   metronome beat: "Fast, simple, powerful", "parse, validate, and
   flatten" as decoration. Real enumerations of real things keep their real
   length; decorative triads get cut to the one word that matters.
3. **Overused buzzwords.** delve, robust, elevate, testament, landscape,
   seamless, leverage, empower, unlock, journey (metaphorical), cutting-edge,
   state-of-the-art, game-changing, holistic, synergy. Use the plain verb:
   read, strong, improve, shows, area, works with, use, let.
4. **The em dash habit.** Em dashes used as a crutch to bolt explanatory
   clauses onto sentences, several per paragraph. Most of them are a comma,
   a period, or parentheses. Budget: an em dash is fine occasionally; two in
   one paragraph is the tell firing. A bullet that defines a term uses a
   colon inside the bold, never a dash: `- **Change events:** text`, not
   `- **Change events** — text`.
5. **Vague transitions.** Corporate filler openings: "In today's fast-paced
   digital world", "We stand at an inflection point", "As the healthcare
   landscape evolves". Open with the subject of the section.

## How to write instead

Short sentences. Concrete nouns and numbers over adjectives. Say who does
what. If a sentence still reads fine after deleting a clause, the clause was
decoration. Prefer "the runner refuses the case with a `literal-grammar`
finding" over any sentence about what the runner "is designed to" do.

## Enforcement

Review-enforced (prose has no lint). New prose is held to this rule from the
first commit in this repository.
