# SciFlow

There is a fundamental other valuable asset in modern science other than that
paper. It is a reproducible, reusable set of project data.

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

    sci pull  # pull in all the project data.


## Supported Remotes

 - [x] FigShare
 - [ ] Data Dryad
 - [ ] Zenodo
 - [ ] static remotes (i.e. just URLs)

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
remote. Most core logic should live here.


## TODO

 - wrap git, do something like `scf clone` that pulls in Git repo.
 - recursive pulling.


