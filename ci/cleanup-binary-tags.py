#!/usr/bin/env python3

import os
import subprocess
import semver


def tag_to_version(tag):
    return tag.split('-')[1].lstrip('v')


subprocess.check_call('git fetch --tags', shell=True)
tags = subprocess.check_output(
    'git tag --list | grep binary', shell=True).decode('UTF-8').splitlines()
versions = sorted(list(set([tag_to_version(tag) for tag in tags])),
                  key=semver.parse_version_info)
versions_to_delete = versions[:-3]

cmd_delete_local = 'git tag --delete'
cmd_delete_remote = 'git push --delete '
GITHUB_TOKEN = os.environ.get('GITHUB_TOKEN')
if GITHUB_TOKEN:
    cmd_delete_remote += (
        'https://{}@github.com/autozimu/LanguageClient-neovim.git'
        .format(GITHUB_TOKEN))
else:
    cmd_delete_remote += 'origin'
for tag in tags:
    if tag_to_version(tag) in versions_to_delete:
        cmd_delete_local += ' ' + tag
        cmd_delete_remote += ' ' + tag

if not cmd_delete_local.endswith('delete'):
    subprocess.check_call(cmd_delete_local, shell=True)
if not (cmd_delete_remote.endswith('origin') or
        cmd_delete_remote.endswith('.git')):
    subprocess.check_call(cmd_delete_remote, shell=True)
