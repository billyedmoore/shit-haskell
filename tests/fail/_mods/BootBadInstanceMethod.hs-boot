module BootBadInstanceMethod where
class Check a where
  check :: a -> a
instance Check Int where
  check = id
