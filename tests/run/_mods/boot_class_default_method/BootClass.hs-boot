module BootClass where
class Select a where
  select :: a -> a
  select = notInScopeInBoot
