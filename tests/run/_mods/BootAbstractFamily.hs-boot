{-# LANGUAGE TypeFamilies #-}
module BootAbstractFamily where

import Data.Kind (Type)

type family F (a :: Type) :: Type where
  ..
