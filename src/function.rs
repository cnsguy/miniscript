#[derive(Debug, Clone)]
pub struct FunctionPrototype {
    num_args: usize,
    chunk: usize,
}

impl FunctionPrototype {
    pub fn new(num_args: usize, chunk: usize) -> Self {
        Self { num_args, chunk }
    }

    pub fn num_args(&self) -> usize {
        self.num_args
    }

    pub fn chunk(&self) -> usize {
        self.chunk
    }
}
