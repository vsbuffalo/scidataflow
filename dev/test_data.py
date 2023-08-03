import numpy as np
import pandas as pd
import string
import random
import shutil
import pytest
import logging
import os

random.seed(0)
logging.basicConfig(level=logging.INFO, format='[sdf-test/%(levelname)s] %(message)s')

test_dir = 'test_project/'

test_data_files = ["data/in.tsv",
                   "data/raw/data_1.tsv",
                   "data/raw/data_2.tsv",
                   "data/raw/data_3.tsv.gz",
                   "data/supplement/figure_1.tsv"]

untracked_data_files = ["data/untracked_file.tsv"]


change_files = ["data/in.tsv", "data/raw/data_2.tsv"]
touch_files = ["data/supplement/figure_1.tsv"]


def ensure_file_path(file_path):
    """
    Create directories for the specified file path if they do not exist.
    Return the filename at the end of the path.
    """
    dir = os.path.dirname(file_path)
    if not os.path.exists(dir):
        os.makedirs(dir)
    return file_path


def write_random_tsv(file_path, rows=10, columns=5):
    """
    Write a random numeric TSV file with random string column
    headers.
    """
    headers = [''.join(random.choices(string.ascii_uppercase + string.ascii_lowercase, k=5))
               for _ in range(columns)]
    data = np.random.rand(rows, columns)
    df = pd.DataFrame(data, columns=headers)
    if file_path.endswith('.gz'):
        df.to_csv(file_path, sep='\t', index=False, compression='gzip')
    else:
        df.to_csv(file_path, sep='\t', index=False)


if __name__ == "__main__":
    join = os.path.join
    for file in test_data_files:
        print(f"writing {file}...")
        write_random_tsv(ensure_file_path(join(test_dir, file)))
    for file in untracked_data_files:
        write_random_tsv(ensure_file_path(join(test_dir, file)))

    with open(ensure_file_path(join(test_dir, 'README.md')), 'w') as f:
        f.write("## README\nA fake readme.\n")

