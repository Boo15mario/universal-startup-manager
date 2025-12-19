.PHONY: srpm

spec ?= universal-startup-manager.spec
outdir ?= .

name := $(shell rpmspec -q --qf '%{name}' $(spec))
version := $(shell rpmspec -q --qf '%{version}' $(spec))
tarball := $(outdir)/$(name)-$(version).tar.gz

srpm:
	mkdir -p $(outdir)
	git archive --format=tar.gz --prefix=$(name)-$(version)/ -o $(tarball) HEAD
	rpmbuild -bs --define "_sourcedir $(outdir)" --define "_srcrpmdir $(outdir)" $(spec)
