pub(super) trait LevelHost {
    fn block_state(&self, _: [i32; 3]) -> Option<super::BlockState> {
        None
    }
    fn state_block(&self, _: BlockStateId) -> Option<&str> {
        None
    }
}
