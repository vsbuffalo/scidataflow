![CI tests](https://github.com/vsbuffalo/sciflow/workflows/CI/badge.svg)

![SciDataFlow logo](https://github.com/vsbuffalo/sciflow/blob/a477fc3a7e612ff4c5d89f3b43e2826b8c90f3b8/logo.png)

# SciDataFlow — Facilitating the Flow of Data in Science

**Problem 1**: Have you ever wanted to reuse and build upon a research
project's output or supplementary data, but can't find it?

**SciDataFlow** solves this issue by making it easy to **unite** a research
project's *data* with its *code*. Often, code for open computational projects
is managed with Git and stored on a site like GitHub. However, a lot of
scientific data is too large to be stored on these sites, and instead is hosted
by sites like  [Zenodo](http://zenodo.org) or [FigShare](http://figshare.com). 

**Problem 2**: Does your computational project have dozens or even hundreds of
intermediate data files you'd like to keep track of? Do you want to see if
these files are changed by updates to computational pipelines.

SciDataFlow also solves this issue by keeping a record of the necessary
information to track when data is changed. This is stored alongside the
information needed to retrieve data from and push data to remote data
repositories. All of this is kept in a simple [YAML](https://yaml.org) "Data
Manifest" (`data_manifest.yml`) file that SciDataFlow manages. This file is
stored in the main project directory and meant to be checked into Git, so that
Git commit history can be used to see changes to data. The Data Manifest is a
simple, minimal, human and machine readable specification. But you don't need
to know the specifics — the simple `sdf` command line tool handles it all for
you.

## A Simple Workflow Example

The user interacts with the Data Manifest through the fast and concurrent
command line tool `sdf` written in the inimitable [Rust
language](https://www.rust-lang.org). The `sdf` tool has a Git-like interface.
If you know Git, using it will be easy, e.g. to initialize SciDataFlow for a
project you'd use:

```console
$ sdf init
```

Registering a file in the manifest: 

```console
$ sdf add data/population_sizes.tsv
Added 1 file.
```

Checking to see if a file has changed, we'd use `sdf status`:

```console
$ sdf status
Project data status:
0 files on local and remotes (1 file only local, 0 files only remote), 1 file total.

[data]
 population_sizes.tsv      current      3fba1fc3      2023-09-01 10:38AM (53 seconds ago)
```

Now, let's imagine a pipeline runs and changes this file: 

```console
$ bash tools/computational_pipeline.sh # changes data
$ sdf status 
Project data status:
0 files on local and remotes (1 file only local, 0 files only remote), 1 file total.

[data]
 population_sizes.tsv      changed      3fba1fc3 → 8cb9d10b        2023-09-01 10:48AM (1 second ago)

```

If these changes are good, we can tell the Data Manifest it should update it's
record of this version:

```console 
$ sdf update data/population_sizes.tsv
$ sdf status
Project data status:
0 files on local and remotes (1 file only local, 0 files only remote), 1 file total.

[data]
 population_sizes.tsv      current      8cb9d10b      2023-09-01 10:48AM (6 minutes ago)

```

**⚠️Warning**: SciDataFlow does not do data *versioning*. Unlike Git, it does
not keep an entire history of data at each commit. Thus, **data backup must be
managed by separate software**. SciDataFlow is still in alpha phase, so it is
especially important you backup your data *before* using SciDataFlow. A tiny,
kind reminder: you as a researcher should be doing routine backups *already* —
losing data due to either a computational mishap or hardware failure is always
possible. 

## Pushing Data to Remote Repositories

SciDataFlow also saves researchers' time when submitting supplementary data to
services like Zenodo or FigShare. Simply link the remote service (you'll need
to first get an API access token from their website):

```console
$ sdf link  data/ zenodo <TOKEN> --name popsize_study
```

You only need to link a remote once. SciDataFlow will look for a project on the
remote with this name first (see `sdf link --help` for more options).
SciDataFlow stores the authentication keys for all remotes in
`~/.scidataflow_authkeys.yml` so you don't have to remember them.

SciDataFlow knows you probably don't want to upload *every* file that you're
keeping track of locally. Sometimes you just want to use SciDataFlow to track
local changes. So, in addition to files being registered in the Data Manifest,
you can also tell them you'd like to *track* them:

```console
$ sdf track data/population_sizes.tsv
```

Now, you can check the status on remotes too with:

```console
$ sdf status --remotes
Project data status:
1 file local and tracked by a remote (0 files only local, 0 files only remote), 1 file total.

[data > Zenodo]
 population_sizes.tsv      current, tracked      8cb9d10b      2023-09-01 10:48AM (14 minutes ago)      not on remote
```

Then, to upload these files to Zenodo, all we'd do is:

```console
$ ../target/debug/sdf push
Info: uploading file "data/population_sizes.tsv" to Zenodo
Uploaded 1 file.
Skipped 0 files.
```

## Retrieving Data from Remotes

A key feature of SciDataFlow is that it can quickly reunite a project's *code*
repository with its *data*. Imagine a colleague had a small repository
containing the code lift a recombination map over to a new reference genome,
and you'd like to use her methods. However, you also want to check that you can
reproduce her pipeline on your system, which first involves re-downloading all
the input data (in this case, the original recombination map and liftover
files).

First, you'd clone the repository: 

```console
$ git clone git@github.com:mclintock/maize_liftover
$ cd maize_liftover/
```

Then, as long as a `data_manifest.yml` exists in the root project directory
(`maize_liftover/` in this example), SciDataFlow is initialized. You can verify
this by using:

```console
$ sdf status  --remotes
Project data status:
1 file local and tracked by a remote (0 files only local, 0 files only remote), 1 file total.

[data > Zenodo]
 recmap_genome_v1.tsv      deleted, tracked      7ef1d10a            exists on remote
 recmap_genome_v2.tsv      deleted, tracked      e894e742            exists on remote
```

Now, to retrieve these files, all you'd need to do is: 

```console
$ sdf pull 
Downloaded 1 file.
 - population_sizes.tsv
Skipped 0 files. Reasons:
```

Note that if you run `sdf pull` again, it will not redownload the file (this is
to over overwriting the local version, should it have been changed): 

```console
$ sdf pull
No files downloaded.
Skipped 1 files. Reasons:
  Remote file is indentical to local file: 1 file
   - population_sizes.tsv
```

If the file has changed, you can pull in the remote's version with `sdf pull
--overwrite`. However, `sdf pull` is also lazy; it will not download the file
if the MD5s haven't changed between the remote and local versions. 

Downloads with SciDataFlow are fast and concurrent thanks to the [Tokio Rust
Asynchronous Universal download MAnager](https://github.com/rgreinho/trauma)
crate. If your project has a lot of data across multiple remotes, SciDataFlow
will pull all data in as quickly as possible. 

## Retrieving Data from Static URLs

Often we also want to retrieve data from URLs. For example, many genomic
resources are available for download from the [UCSC](http://genome.ucsc.edu) or
[Ensembl](http://ensembl.org) websites as static URLs. We want a record of
where these files come from in the Data Manifest, so we want to combine a
download with a `sdf add`. The command `sdf get` does this all for you — let's
imagine you want to get all human coding sequences. You could do this with: 

```console
$ sdf get https://ftp.ensembl.org/pub/release-110/fasta/homo_sapiens/cds/Homo_sapiens.GRCh38.cds.all.fa.gz
⠄ [================>                       ] 9639693/22716351 (42%) eta 00:00:08
```

Now, it would show up in the Data Manifest:

```console
$ sdf status --remotes
Project data status:
0 files local and tracked by a remote (0 files only local, 0 files only remote), 1 files total.

[data > Zenodo]
 Homo_sapiens.GRCh38.cds.all.fa.gz      current, untracked      fb59b3ad      2023-09-01  3:13PM (43 seconds ago)      not on remote
```

Note that files downloaded from URLs are not automatically track with remotes.
You can do this with `sdf track <FILENAME>` if you want. Then, you can use `sdf
push` to upload this same file to Zenodo or FigShare. 

Since modern computational projects may require downloading potentially
*hundreds* or even *thousands* of annotation files, the `sdf` tool has a simple
way to do this: tab-delimited or comma-separated value files (e.g. those with
suffices `.tsv` and `.csv`, respectively). The big picture idea of SciDataFlow
is that it should take mere seconds to pull in all data needed for a large
genomics project (or astronomy, or ecology, whatever). Here's an example TSV
file full of links:

```console
$ cat human_annotation.tsv
type	url
cdna	https://ftp.ensembl.org/pub/release-110/fasta/homo_sapiens/cdna/Homo_sapiens.GRCh38.cdna.all.fa.gz
fasta	https://ftp.ensembl.org/pub/release-110/fasta/homo_sapiens/dna/Homo_sapiens.GRCh38.dna.alt.fa.gz
cds	https://ftp.ensembl.org/pub/release-110/fasta/homo_sapiens/cds/Homo_sapiens.GRCh38.cds.all.fa.gz
```

Note that this has a header, and the URLs are in the second column. To get this data, we'd use: 

```console
$ sdf bulk human_annotation.tsv --column 1 --header
⠁ [                                        ] 0/2 (0%) eta 00:00:00
⠉ [====>                                   ] 9071693/78889691 (11%) eta 00:01:22
⠐ [=========>                              ] 13503693/54514783 (25%) eta 00:00:35
```

**Columns indices are zero-indexed** and `sdf bulk` assumes no headers by
default. Note that in this example, only two files are downloading — this is
because `sdf` detected the CDS file already existed. SciDataFlow tells you this
with a little message at the end: 

```console 
$ sdf bulk human_annotation.tsv --column 1 --header
3 URLs found in 'human_annotation.tsv.'
2 files were downloaded, 2 added to manifest (0 were already registered).
1 files were skipped because they existed (and --overwrite was no specified).
```

## Adding Metadata

Some data repository services like Zenodo allow data depositions to be
associated with a creator's metadata (e.g. full name, email, affiliation).
SciDataFlow automatically propagates this from a file in
`~/.scidataflow_config`. You can set your user metadata (which should be done
early on, sort of like with Git) with:

```console
$ sdf config --name "Joan B. Scientist" --email "joanbscientist@berkeley.edu" --affiliation "UC Berkeley"
```

Projects can also have store metadata, such as a title and description. This is
kept in the Data Manifest. You can set this manually with: 

```console
$ sdf metadata --title "genomics_analysis" --description "A re-analysis of Joan's data."
```

## SciDataFlow's Vision

The larger vision of SciDataFlow is to change how data flows through scientific
projects. The way scientific data is currently shared is fundamentally
**broken**, which prevents the reuse of data that is the output of some smaller
step in the scientific process. We call these **scientific assets**. 

**Scientific Assets** are the output of some computational pipeline or analysis
which has the following important characteristic:  **Scientific Assets should
be *reusable* by *everyone*, and be *reused* by everyone.** Being **reusable**
means all other researchers should be *able* to quickly reuse a scientific
asset (without having to spend hours trying to find and download data). Being
**reused** by everyone means that using a scientific asset should be the *best*
way to do something. 

For example, if I lift over a recombination map to a new reference genome, that
pipeline and output data should be a scientific asset. It should be reusable to
everyone — we should **not** each be rewriting the same bioinformatics
pipelines for routine tasks. There are three problems with this: (1) each
reimplementation has an independent chance of errors, (2) it's a waste of time,
(3) there is no cumulative *improvement* of the output data. It's not an
*asset*; the result of each implementation is a *liability*!

Lowering the barrier to reusing computational steps is one of SciDataFlow's
main motivations. Each scientific asset should have a record of what
computational steps produced output data, and with one command (`sdf pull`) it
should be possible to retrieve all data outputs from that repository. If the
user only wants to reuse the data, they can stop there — they have the data
locally and can proceed with their research. If the user wants to investigate
how the input data was generated, the code is right there too. If they want to
try rerunning the computational steps that produced that analysis, they can do
that too. Note that SciDataFlow is agnostic to this — by design, it does not
tackle the hard problem of managing software versions, computational
environments, etc. It can work alongside software (e.g.
[Docker](https://www.docker.com) or
[Singularity](https://docs.sylabs.io/guides/3.5/user-guide/introduction.html#))
that tries to solve that problem.

By lowering the barrier to sharing and retrieving scientific data, SciDataFlow
hopes to improve the reuse of data. 

## Future Plans

In the long run, the SciDataFlow YAML specification would allow for recipe-like
reuse of data. I would like to see, for example, a set of human genomics
scientific assets on GitHub that are continuously updated and reused. Then,
rather than a researcher beginning a project by navigating many websites for
human genome annotation or data, they might do something like:

```console
$ mkdir -p new_adna_analysis/data/annotation
$ cd new_adna_analysis/data/annotation
$ git clone git@github.com:human_genome_assets/decode_recmap_hg38
$ (cd decode_recmap/ && sdf pull)
$ git clone git@github.com:human_genome_assets/annotation_hg38
$ (cd annotation_hg38 && sdf pull)
```

and so forth. Then, they may look at the `annotation_hg38/` asset, find a
problem, fix it, and issue a GitHub pull request. If the change is fixed, the
maintainer would then just do `sdf push --overwrite` to push the data file to
the data repository. Then, the Scientific Asset is then updated for everyone to
use an benefit from. All other researchers can then instantly use the updated
asset; all it takes is a mere `sdf pull --overwrite`.

## Installing SciDataFlow

To install the SciDataFlow tool `sdf` from source, you'll first need to install
the Rust Programming Language. See this page for more info, but if you just
want to get up and running, you can run: 

```
$ curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

Then, to install SciDataFlow, just run:

```console 
$ cargo install scidataflow
```

To test, just try running `sdf --help`.

## Reporting Bugs

If you are a user of SciDataFlow and encounter an issue, please submit an issue
to
[https://github.com/vsbuffalo/scidataflow/issues](https://github.com/vsbuffalo/scidataflow/issues)!

## Contributing to SciDataFlow

If you are a Rust developer, **please** contribute! Here are some great ways to
get started:

 - Write some API tests. See some of the tests in `src/lib/api/zenodo.api` as
   an example.

 - Write some integration tests. See `tests/test_project.rs` for examples.

 - A cleaner error framework. Currently SciDataflow uses
   [anyhow](https://crates.io/crates/anyhow), which works well, but it would be
   nice to have more specific error `enums`. 

 - Improve the documentation!

