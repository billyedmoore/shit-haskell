{-# LANGUAGE TypeFamilyDependencies #-}

import Data.Kind (Type)

type family F (a :: k) = (r :: Type) | r -> k
type family G a = (r :: k) | r -> k
type family H a = r

main :: IO ()
main = pure ()
