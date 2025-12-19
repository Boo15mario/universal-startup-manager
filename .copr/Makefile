.PHONY: srpm

spec ?= universal-startup-manager.spec
outdir ?= .

name := $(shell rpmspec -q --qf '%{name}' $(spec))
version := $(shell rpmspec -q --qf '%{version}' $(spec))
tarball := $(outdir)/$(name)-$(version).tar.gz

srpm:
	mkdir -p $(outdir)
	tar --exclude-vcs --exclude='./target' --exclude='./fedora-*' \
		--transform "s|^|$(name)-$(version)/|" \
		-czf $(tarball) .
	rpmbuild -bs --define "_sourcedir $(outdir)" --define "_srcrpmdir $(outdir)" $(spec)
