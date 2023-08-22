![CI tests](https://github.com/vsbuffalo/sciflow/workflows/CI/badge.svg)

# SciFlow -- Facilitating the Flow of Data in Science

SciFlow is a both (1) a minimal specification of the data used in a scientific
project and (2) a fast, concurrent command line tool to retrieve and upload
scientific data from multiple repositories (e.g. FigShare, Zenodo, etc.).

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
 - [ ] Zenodo
 - [ ] static remotes (i.e. just URLs)

## TODO 

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


