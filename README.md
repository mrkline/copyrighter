Copyrighter uses Git history and existing copyright notices to generate updated
ones for files.

## Example usage

To update all .cpp and .h files in a project,
```
$ cd my_project
$ find -type f \( -name '*.cpp' -or -name '*.h'\) \
    -exec copyrighter --organization "Fluke Corporation. All rights reserved." {} +
```

I'll probably make a script (or another program) that automates this process.

## Why?

As of our last discussion with them, Legal demands that all code files contain
a copyright notice, complete with each year the file was modified.
This work is far too menial for humans.

## How?

See [git-historian](https://github.com/mrkline/git-historian) for the Git history
side of things. The rest is fairly straightforward file manipulation.

## Why Rust?

[Because](https://www.youtube.com/watch?v=_-fweBvtifA) [it's awesome](http://www.smbc-comics.com/?id=2088)
(and I wanted to try it out for a Real™ project).
