{-# LANGUAGE TypeFamilyDependencies, NoKindSignatures #-}

import Data.Kind (Type)

type family F a = (r :: Type) | r -> a

main :: IO ()
main = pure ()
