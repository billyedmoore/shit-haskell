module BootClass where
import BootUser ()
class Select a where
  select :: a -> a
  select = id
