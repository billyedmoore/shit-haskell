{-# LANGUAGE TypeFamilies #-}
module BootOpenFamilyClosed where

import Data.Kind (Type)

type family F (a :: Type) :: Type
