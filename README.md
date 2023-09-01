![CI tests](https://github.com/vsbuffalo/sciflow/workflows/CI/badge.svg)

![SciDataFlow logo](https://github.com/vsbuffalo/sciflow/blob/a477fc3a7e612ff4c5d89f3b43e2826b8c90f3b8/logo.png)

# SciDataFlow — Facilitating the Flow of Data in Science

**Problem 1**: Have you ever wanted to reuse and build upon a research project's output or supplementary data, but can't find it?

**SciDataFlow** solves this issue by making it easy to **unite** a research project's *data* with its *code*. Often, code for open computational projects is managed with Git and stored on a site like GitHub. However, a lot of scientific data is too large to be stored on these sites, and instead is hosted by sites like  [Zenodo](http://zenodo.org) or [FigShare](http://figshare.com). 

**Problem 2**: Does your computational project have dozens or even hundreds of intermediate data files you'd like to keep track of? Do you want to see if these files are changed by updates to computational pipelines.

SciDataFlow also solves this issue by keeping a record of the necessary information to track when data is changed. This is stored alongside the information needed to retrieve data from and push data to remote data repositories. All of this is kept in a simple [YAML](https://yaml.org) "Data Manifest" (`data_manifest.yml`) file that SciDataFlow manages. This file is stored in the main project directory and meant to be checked into Git, so that Git commit history can be used to see changes to data. The Data Manifest is a simple, minimal, human and machine readable specification. But you don't need to know the specifics — the simple `sdf` command line tool handles it all for you.

## A Simple Workflow Example

The user interacts with the Data Manifest through the fast and concurrent command line tool `sdf` written in the inimitable [Rust language](https://www.rust-lang.org). The `sdf` tool has a Git-like interface. If you know Git, using it will be easy, e.g. to initialize SciDataFlow for a project you'd use:

```bash
$ sdf init
```

Registering a file in the manifest: 

```bash
$ sdf add data/population_sizes.tsv
Added 1 file.
```

Checking to see if a file has changed, we'd use `sdf status`:

```bash
$ sdf status
Project data status:
0 files on local and remotes (1 file only local, 0 files only remote), 1 file total.

[data]
 population_sizes.tsv      current      3fba1fc3      2023-09-01 10:38AM (53 seconds ago)
```

Now, let's imagine a pipeline runs and changes this file: 

```bash
$ bash tools/computational_pipeline.sh # changes data
$ sdf status 
Project data status:
0 files on local and remotes (1 file only local, 0 files only remote), 1 file total.

[data]
 population_sizes.tsv      changed      3fba1fc3 → 8cb9d10b        2023-09-01 10:48AM (1 second ago)

```

If these changes are good, we can tell the Data Manifest it should update it's record of this version:

```bash 
$ sdf update data/population_sizes.tsv
$ sdf status
Project data status:
0 files on local and remotes (1 file only local, 0 files only remote), 1 file total.

[data]
 population_sizes.tsv      current      8cb9d10b      2023-09-01 10:48AM (6 minutes ago)

```

**⚠️Warning**: SciDataFlow does not do data *versioning*. Unlike Git, it does not keep an entire history of data at each commit. Thus, **data backup must be managed by separate software**. SciDataFlow is still in alpha phase, so it is especially important you backup your data *before* using SciDataFlow. A tiny, kind reminder: you as a researcher should be doing routine backups *already* — losing data due to either a computational mishap or hardware failure is always possible. 

## Pushing Data to Remote Repositories

SciDataFlow also saves researchers' time when submitting supplementary data to services like Zenodo or FigShare. Simply link the remote service (you'll need to first get an API access token from their website):

```bash
$ sdf link  data/ zenodo <TOKEN> --name popsize_study
```

You only need to link a remote once. SciDataFlow will look for a project on the remote with this name first (see `sdf link --help` for more options). SciDataFlow stores the authentication keys for all remotes in `~/.scidataflow_authkeys.yml` so you don't have to remember them.

SciDataFlow knows you probably don't want to upload *every* file that you're keeping track of locally. Sometimes you just want to use SciDataFlow to track local changes. So, in addition to files being registered in the Data Manifest, you can also tell them you'd like to *track* them:

```bash
$ sdf track data/population_sizes.tsv
```

Now, you can check the status on remotes too with:

```bash
$ sdf status --remotes
Project data status:
1 file local and tracked by a remote (0 files only local, 0 files only remote), 1 file total.

[data > Zenodo]
 population_sizes.tsv      current, tracked      8cb9d10b      2023-09-01 10:48AM (14 minutes ago)      not on remote
```

Then, to upload these files to Zenodo, all we'd do is:

```bash
$ ../target/debug/sdf push
Info: uploading file "data/population_sizes.tsv" to Zenodo
Uploaded 1 file.
Skipped 0 files.
```

## Retrieving Data from Remotes

A key feature of SciDataFlow is that it can quickly reunite a project's *code* repository with its *data*. Imagine a colleague had a small repository containing the code lift a recombination map over to a new reference genome, and you'd like to use her methods. However, you also want to check that you can reproduce her pipeline on your system, which first involves re-downloading all the input data (in this case, the original recombination map and liftover files).

First, you'd clone the repository: 

```bash
$ git clone git@github.com:mclintock/maize_liftover
$ cd maize_liftover/
```

Then, as long as a `data_manifest.yml` exists in the root project directory (`maize_liftover/` in this example), SciDataFlow is initialized. You can verify this by using:

```bash
$ sdf status  --remotes
Project data status:
1 file local and tracked by a remote (0 files only local, 0 files only remote), 1 file total.

[data > Zenodo]
 recmap_genome_v1.tsv      deleted, tracked      7ef1d10a            exists on remote
 recmap_genome_v2.tsv      deleted, tracked      e894e742            exists on remote
```

Now, to retrieve these files, all you'd need to do is: 

```bash
$ sdf pull 
Downloaded 1 file.
 - population_sizes.tsv
Skipped 0 files. Reasons:
```

Note that if you run `sdf pull` again, it will not redownload the file (this is to over overwriting the local version, should it have been changed): 

```bash
$ sdf pull
No files downloaded.
Skipped 1 files. Reasons:
  Remote file is indentical to local file: 1 file
   - population_sizes.tsv
```

If the file has changed, you can pull in the remote's version with `sdf pull --overwrite`. However, `sdf pull` is also lazy; it will not download the file if the MD5s haven't changed between the remote and local versions. 

Downloads with SciDataFlow are fast and concurrent thanks to the [Tokio Rust Asynchronous Universal download MAnager](https://github.com/rgreinho/trauma) crate. If your project has a lot of data across multiple remotes, SciDataFlow will pull all data in as quickly as possible. 

## Retrieving Data from Static URLs

Often we also want to retrieve data from URLs. For example, many genomic resources are available for download from the [UCSC](http://genome.ucsc.edu) or [Ensembl](http://ensembl.org) websites as static URLs. We want a record of where these files come from in the Data Manifest, so we want to combine a download with a `sdf add`. The command `sdf get` does this all for you — let's imagine you want to get all human coding sequences. You could do this with: 

```bash
$ sdf get https://ftp.ensembl.org/pub/release-110/fasta/homo_sapiens/cds/Homo_sapiens.GRCh38.cds.all.fa.gz
⠄ [================>                       ] 9639693/22716351 (42%) eta 00:00:08
```

Now, it would show up in the Data Manifest:

```bash
$ sdf status --remotes
Project data status:
0 files local and tracked by a remote (0 files only local, 0 files only remote), 1 files total.

[data > Zenodo]
 Homo_sapiens.GRCh38.cds.all.fa.gz      current, untracked      fb59b3ad      2023-09-01  3:13PM (43 seconds ago)      not on remote
```

Note that files downloaded from URLs are not automatically track with remotes. You can do this with `sdf track <FILENAME>` if you want. Then, you can use `sdf push` to upload this same file to Zenodo or FigShare. 

Since modern computational projects may require downloading potentially *hundreds* or even *thousands* of annotation files, the `sdf` tool has a simple way to do this: tab-delimited or comma-separated value files (e.g. those with suffices `.tsv` and `.csv`, respectively). The big picture idea of SciDataFlow is that it should take mere seconds to pull in all data needed for a large genomics project (or astronomy, or ecology, whatever). Here's an example TSV file full of links:

```bash
$ cat human_annotation.tsv
type	url
cdna	https://ftp.ensembl.org/pub/release-110/fasta/homo_sapiens/cdna/Homo_sapiens.GRCh38.cdna.all.fa.gz
fasta	https://ftp.ensembl.org/pub/release-110/fasta/homo_sapiens/dna/Homo_sapiens.GRCh38.dna.alt.fa.gz
cds	https://ftp.ensembl.org/pub/release-110/fasta/homo_sapiens/cds/Homo_sapiens.GRCh38.cds.all.fa.gz
```

Note that this has a header, and the URLs are in the second column. To get this data, we'd use: 

```bash
$ sdf bulk human_annotation.tsv --column 1 --header
⠁ [                                        ] 0/2 (0%) eta 00:00:00
⠉ [====>                                   ] 9071693/78889691 (11%) eta 00:01:22
⠐ [=========>                              ] 13503693/54514783 (25%) eta 00:00:35
```

**Columns indices are zero-indexed** and `sdf bulk` assumes no headers by default. Note that in this example, only two files are downloading — this is because `sdf` detected the CDS file already existed. SciDataFlow tells you this with a little message at the end: 

```bash 
$ sdf bulk human_annotation.tsv --column 1 --header
3 URLs found in 'human_annotation.tsv.'
2 files were downloaded, 2 added to manifest (0 were already registered).
1 files were skipped because they existed (and --overwrite was no specified).
```

### Adding Metadata

Some data repository services like Zenodo support 

This indicates the 
The larger vision of SciDataFlow is to change how data flows through scientific projects. The way scientific data is currently shared is fundamentally **broken**, and it is prevent the reuse of important **scientific assets**. By lowering the barrier to sharing and retrieving scientific data, SciDataFlow hopes to improve the *reuse* of data. For example, suppose one of your projects required 

Or perhaps you're in
the midst of a large computational project, and need a better way to track the
data going into a project, intermediate data, and whether data has changed
during different runs of a pipeline.

Perhaps you're submitting a manuscript to a journal, and you need to go through
a large project's directory to find and upload supplementary data to a data
repository service like [Zenodo](http://zenodo.org) or
[FigShare](http://figshare.com). It can be labor intensive to manually find and
upload each of these data files necessary to make a project reproducible.

Or maybe a colleague created a small but important *scientific asset*, such as
lifting a recombination map to a new reference genome versionl



## Philosophy 

There is a fundamental other valuable asset in modern science other than that
paper. It is a reproducible, reusable set of project data.

SciFlow uses a fairly permissive project structure. However, it must 
work around one key issue: most remote data stores do not supported
nested hierarchy. Of course, one could just archive and zip a complex 
data directory (and in some cases, that is the only option). However,
keeping this updated is a pain, and it archive files (like `.tar.gz`)
obscure the data files they contain, making it difficult to track
them and their changes.

SciFlow gets around this by allowing data to be in any directory below the
project root directory (the directory containing `data_manifest.yml`, and
likely `.git/`). However, t



Science has several problems, and I think the most severe of which is that
manuscripts have extremely limited value in terms of our long-term
understanding of complex phenomenon. Science needs to build of previous work
organically, so that lineages of work on one topic can rely on the same,
robust shared assets.

What is ProjectData? 

ProjectData are sort of like supplementary material, except instead of being
stored in a PDF that is hard to access, it is immediately accessible from
anywhere with the commands:

    git clone https://github.com/scientist_anne/research_project.git
    cd research_project/
    scf status

    scf pull  # pull in all the project data.


## Supported Remotes

 - [x] FigShare
 - [ ] Data Dryad
 - [x] Zenodo
 - [ ] static remotes (i.e. just URLs)

## TODO 

 - remote_init for zenodo needs to check for existing.

 - link_only should propagate remote IDs, etc

 - we need to be more strict about whether the remotes have files that 
   are listed as tracked in *subdirectories*. E.g. we should, when a 
   link to a remote is added to track a directory, check that that
   directory does not have files in the manifest that are are in 
   subdirectories.

## Operation

Digest states:

 - local file system
 - local manifest
 - remote supports MD5 
 - remote does not supports MD5 

1. Pulling in remote files.

2. Pushing local files to a remote.

3. Clobbered local data. A more complex case; local data is "messy", e.g. local
   files and manifest disagree. 






## Design

The main `struct`s for managing the data manifest are `DataFile` (corresponding
to one registered data file) and `DataCollection`. `DataCollection` stores a
`HashMap` with keys that are the *relative* path to the (relative to where
`data_manifest.yml` is) and values that are the `DataFile` object. `serde`
manages serialization/deserialization. `DataCollection` also manages the remote
types (e.g. FigShare, Data Dryad, etc). Each remote is stored in a `HashMap`
with the path tracked by the remote as keys, and a `Remote` `enum` for the
values. Each `Remote` `enum` corresponds to one of the supported remotes.

The `Remote` `enum` has methods that are meant as a generic interface for *any*
remote. Most core logic should live here. Furthermore, `DataCollection` and `DataFile`


## Statuses

Files registered in the data manifest can have multiple statuses: 

 - **Local status**: 
   - **Current**: digest agrees between manifest and the local file.

   - **Modified**: digest disagrees between manifest and the local file, i.e.
     it has been modified.
      
   - **Deleted**: A record of this data file exists in the manifest, but the
     file does not exist.

   - **Invalid**: Invalid state.

 - **Remote status**: 


 - **Tracked**: whether the local data file is to be synchronized with remotes.

 - **Local-Remote MD5 mismatch**: 

In data manifest, not tracked: upon push, will not be uploaded to remote. If
it's on the remote, this should prompt an error.






## TODO

 - wrap git, do something like `scf clone` that pulls in Git repo.
 - recursive pulling.
 - check no external file; add tests


