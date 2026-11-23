{-# LANGUAGE TypeFamilies #-}
module BootAbstractFamilyOpen where

import Data.Kind (Type)

type family F (a :: Type) :: Type
