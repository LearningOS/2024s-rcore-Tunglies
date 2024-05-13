cd ../ci-user
make test CHAPTER=6 LOG=DEBUG
cd -
git restore Makefile build.rs ../easy-fs-fuse/src/main.rs