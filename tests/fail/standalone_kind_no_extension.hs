{-# LANGUAGE NoStandaloneKindSignatures #-}

import Data.Kind (Type)

type T :: Type
data T = T

main = pure ()
