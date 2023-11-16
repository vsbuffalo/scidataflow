![Crates.io](https://img.shields.io/crates/v/scidataflow) ![Crates.io](https://img.shields.io/crates/d/scidataflow) ![CI tests](https://github.com/vsbuffalo/sciflow/workflows/CI/badge.svg)

![SciDataFlow logo](https://github.com/vsbuffalo/sciflow/blob/a477fc3a7e612ff4c5d89f3b43e2826b8c90f3b8/logo.png)


# SciDataFlow — Facilitating the Flow of Data in Science

![SciDataFlow demo screencast](https://github.com/vsbuffalo/scidataflow/blob/6c7294f33498b9d77e4e0b830502b9c0719ed6db/screencast.gif)

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

## Documentation

SciDataFlow has [extensive
documentation](https://vsbuffalo.github.io/scidataflow-doc/) full of
examples of how to use the various subcommands.

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

If you'd like to the Rust Programming Language manually, [see this
page](https://www.rust-lang.org/tools/install), which instructs you to run:

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
get started (also check the TODO list below, or for TODOs in code!):

 - Write some API tests. See some of the tests in `src/lib/api/zenodo.api` as
   an example.

 - Write some integration tests. See `tests/test_project.rs` for examples.

 - A cleaner error framework. Currently SciDataflow uses
   [anyhow](https://crates.io/crates/anyhow), which works well, but it would be
   nice to have more specific error `enums`. 

 - Improve the documentation!

## Todo

 - [] `sdf mv` tests, within different directories.
