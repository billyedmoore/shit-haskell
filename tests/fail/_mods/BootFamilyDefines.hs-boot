{-# LANGUAGE TypeFamilies #-}
module BootFamilyDefines where

type family Result a
type instance Result Int = Bool
