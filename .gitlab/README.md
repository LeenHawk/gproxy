# GitLab mirror

CI builds now run on CNB; see [the CNB configuration](../.cnb/README.md).
`.gitlab-ci.yml` prevents duplicate GitLab pipelines. GitLab continues to mirror
source and hosts the same CNB-built release packages and container images.
The API adapters under `scripts/gitlab/` are reused by CNB for three-host publication.
