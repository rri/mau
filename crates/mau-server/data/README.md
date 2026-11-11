# Landing-page quotes

`quotes.json` holds 1,028 short passages from public-domain science fiction,
one of which the signed-out landing page shows at random on each load
(`GET /api/quote`).

**Source.** English texts on Project Gutenberg's science-fiction shelf whose
every author and translator died in 1955 or earlier, so the works are in the
public domain worldwide (life + 70 years), not only in the United States.

**Selection.** A script picked candidate sentences that stand on their own
(complete sentences of 60–240 characters, general statements, no character
names or dialogue tags), then every quote here was read and chosen by hand.
Lines with dated or demeaning language about race, women or disability were
excluded, as were five works whose themes don't suit a welcome page (The
Yellow Danger, The Fall of a Nation, Cæsar's Column, The Great White Queen,
The Coming Conquest of England). Bible verses quoted inside novels were left
out so they aren't credited to the novelist.

**Fields.** `text`, `author`, `title`, `year` (first publication, given only
where known; otherwise null), `gutenberg` (the ebook number; the page links to
`https://www.gutenberg.org/ebooks/<number>`).
