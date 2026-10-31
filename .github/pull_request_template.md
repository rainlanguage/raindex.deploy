<!--
Merge brief. AI agents and people both follow these rules. Replace every comment, and do not keep this block.

Write for the person who approves the merge, not for someone reading the code. Agents review the code line by line, so the approver does not read the diff.
Every line must help them decide to merge, or know what to watch after the merge. If a reviewer can learn it from the diff, cut it.

Hard limits:
- Budget: about 150 words for low risk, 250 for medium, and 400 for high. Never more than 500 words, not counting links.
- At most 3 bullets per section. Each bullet is one or two lines.
- Delete every section that has nothing real to say, but always keep the Not verified bullet in Proof. A trivial PR is the headline, the slot line, and a Proof section with only the Not verified bullet.

Never include:
- A What, Why, How, or Implementation section.
- A restated diff, or a list of changed files.
- The story of the investigation, or of the review rounds.
- Test counts, or lists of lint, fmt, CI, or review runs. The checks tab shows them.
- A tool-generated summary.

Writing:
- Plain, simple English. One idea per sentence. Short sentences. Active voice with a real subject.
- Describe the change in system terms (behavior, money, services, alerts), not in code terms.
- Say what the code does, not what you hope it does. Never invent evidence.
- Link every issue as a real Markdown link. A bare Linear ID does not link on GitHub.
- Do not hard-wrap lines. Write each sentence or bullet on one line.
-->

<!-- Headline: 1 to 3 sentences, with no heading above them. What the system does after the merge, where its output goes, and why now. Link the issue. In a stack, say which part this is and what the next part adds. -->

**Live effect:** <!-- none, or what changes for users, money, services, or alerts --> · **Risk:** <!-- low, medium, or high, with the triggers in parentheses: money path, keys or IAM, production config or infrastructure, stored data, hard to undo. Name them even for low risk. --> · **Ships:** <!-- on merge, next release, or after a manual step --> · **Blocks:** <!-- PRs that must merge after this one, or delete this slot -->

## Needs your call

<!-- Only if you need a decision from another person. @mention the person who must answer. Never give the PR author an open action here. -->

-

## Decisions

<!-- At most 3. Only choices a teammate could dispute. Write each as "X, not Y, because Z." Link each to its lines in the diff. -->

-

## Risks

<!-- At most 3. What can go wrong, how bad it is, and what limits it or which issue follows up. -->

-

## Proof

<!-- The strongest 1 or 2 pieces of evidence, so that with the Not verified bullet the section stays within 3 bullets: the test that reproduces the bug, a staging check, the plan summary, a screenshot. Not a list of every test. -->

-
- **Not verified:** <!-- Always keep this bullet, also in a trivial PR. Name what nobody checked, human or agent. -->

## Rollout

<!-- Only if live behavior changes or a manual step is needed. Give each step the signal to check. The last step is the rollback, with any trap named. -->

1.
