{-# LANGUAGE TypeFamilies #-}
module BootBadAssociatedFamily where
class Check a where
  type Result a
instance Check Int where
  type Result Int = Bool
