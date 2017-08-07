.PHONY: build-docker-image
build-docker-image: Dockerfile
	docker build --tag autozimu/languageclientneovim .

.PHONY: publish-docker-image
publish-docker-image:
	docker image push autozimu/languageclientneovim

