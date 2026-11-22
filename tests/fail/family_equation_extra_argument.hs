{-# LANGUAGE TypeFamilies #-}

import Data.Kind (Type)

type family Result a :: Type -> Type
type instance Result Int Bool = Char

main = pure ()
