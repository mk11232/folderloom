# Folder merge interaction research

Reviewed official project documentation on 2026-11-07. This is interaction research, not an application audit or a claim that Folderloom implements these behaviors. Upstream documentation is mutable; dupeGuru labels its linked manual 4.0.3.

## What upstream tools establish

| Tool | Documented interaction | Useful lesson for Folderloom |
| --- | --- | --- |
| FreeFileSync | Choose left/right folders, compare, choose synchronization settings, then synchronize. Its main window includes a tree overview, operation preview, category filters and statistics. | Give users a visible source-to-destination plan before execution; summarize folders before individual files. |
| Beyond Compare | Folder Merge presents two folder structures side by side, optionally with a common ancestor, and combines changes into a specified output folder. Its display distinguishes mergeable items and conflicts, and distinguishes automatic from manual actions. | Make the output folder a first-class part of the page. Separate the comparison finding from the action that will be performed. |
| dupeGuru | Results are duplicate groups with a protected reference file and indented duplicate members. A details panel compares the selected file with its reference. Highlighting and marking are distinct; filtered results retain group context. | In duplicate review, show the retained copy and its alternatives together. Keep source paths visible, including when filtering. |
| Czkawka / Krokiet | Krokiet shows bold group headers and member file rows; reference paths are protected. Its Move dialog can copy instead, preserve structure, and rename conflicts instead of skipping. Duplicate Files supports content hashes as well as weaker name/size methods. | Keep duplicate finding separate from folder assembly. Distinguish identical content from a same-name collision, and show structure preservation explicitly. |

Sources: [FreeFileSync quick start](https://freefilesync.org/manual.php?topic=freefilesync), [Beyond Compare Folder Merge](https://www.scootersoftware.com/v5help/viewdirmerge.html), [Beyond Compare display](https://www.scootersoftware.com/v5help/dir_merge_understanding_the_display.html), [dupeGuru results](https://dupeguru.voltaicideas.net/help/en/results.html), [Krokiet guide](https://github.com/qarmin/czkawka/blob/master/instructions/Instruction_Krokiet.md), [Czkawka shared terminology](https://github.com/qarmin/czkawka/blob/master/instructions/Instruction.md).

FreeFileSync also makes an important semantic distinction: Mirror makes the target match the source, including deletions; Update adds/updates without propagating source deletions; Two way propagates changes in both directions. None of these names should be used casually as a synonym for assembling a new combined folder. [Official synchronization settings](https://freefilesync.org/manual.php?topic=synchronization-settings).

## Proposed Folderloom design (recommendation, not upstream behavior)

Start with a plain question: **What do you want to do with these folders?** Give three distinct choices with concrete output examples:

| Choice | Meaning | Example output |
| --- | --- | --- |
| Combine folder contents | Treat selected roots as versions of one collection. Combine matching relative subfolders; retain unique files; review conflicting destinations. | `Drive A/Case/Medical/a.pdf` + `Drive B/Case/Medical/b.pdf` → `Combined/Medical/{a,b}.pdf` |
| Keep each folder separate | Copy each selected root beneath a distinct named child folder. Nested contents retain their relative structure. | Same inputs → `Combined/Drive A Case/Medical/a.pdf` and `Combined/Drive B Case/Medical/b.pdf` |
| Find duplicate files | Compare file content across selected locations and review groups of matching files. This creates no combined folder. | A group shows one retained copy and matching copies with full source paths. |

“Preserve folder structure” alone is ambiguous: both combine and keep-separate can preserve nested paths. Show whether the selected root itself becomes a destination child. If roots have the same basename, show and let the user edit their distinct output names before planning. Combine must never imply flattening every file into one directory.

For folder assembly, show **Source folders → Destination folder**, followed by a small output tree. Source rows should describe folders: name, full location, file count, and planned destination. Do not mix these rows with duplicate-member rows or low-level copy operations.

Make the first review a folder summary: **Ready to copy**, **Already present / identical**, **Needs a decision**, and **Could not inspect**. Expand a folder to inspect file details; provide a dedicated conflict queue. Each conflict shows source paths, proposed destination, sizes/dates, comparison evidence, and the result of each available decision. A same destination path with different content is a conflict; identical content at different paths is a duplicate finding. Do not conflate either with a similar-looking image.

Suggested conflict choices are **Keep both** (preview the exact renamed output), **Use this copy** (identify the selected source), and **Leave out**. Only expose choices that the current planner and executor actually support. Recompute the output preview after decisions. The final action should state its effect, such as **Copy 246 files to Combined**, with source retention stated next to it. Progress and completion should use the same counts and destination names as the preview.

For duplicate-only results, keep the retained file visually distinct and group related files underneath. Clicking a row should inspect it; any checkbox must state what checking means. Avoid an unlabeled checkbox meaning inclusion in one view and deletion in another. Folder-level controls must explain whether they affect the whole folder or only visible members.

## What not to borrow

- Do not lead with Krokiet’s many tool categories or expert hash settings. Its grouped duplicate results are useful inside duplicate review, not as the top-level folder merge interface.
- Do not borrow FreeFileSync’s bidirectional arrows or Mirror terminology for a copy-to-new-folder workflow. Users could reasonably infer updates or deletions to their sources.
- Do not require Beyond Compare’s common-ancestor concept for ordinary folder collection. Its separate inputs/output and explicit conflicts are the relevant patterns.
- Do not inherit dupeGuru’s biggest-file reference heuristic as a universal keeper rule. Size or recency does not establish which document version the user wants.
- Do not use color alone for conflicts or action status; add explicit text. Do not present two selection states without explaining their effect.

The strongest recommendation is to separate **folder assembly** from **duplicate review**, then make the assembly layout visible before asking for file-level decisions. A clearer table alone cannot explain an ambiguous operation.
