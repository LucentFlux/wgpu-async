pipeline {
  agent any
  stages {
    stage('Build') {
      steps {
        sh 'cargo build --package wgpu-async --verbose'
      }
    }

    stage('Test') {
      steps {
        sh 'cargo install cargo2junit'
        sh 'cargo test --package wgpu-async --no-fail-fast -- -Z unstable-options --format json --test-threads 1 --report-time | cargo2junit > results.xml || true'
        archiveArtifacts '**/results.xml'
        junit 'results.xml'
      }
    }
  }
}