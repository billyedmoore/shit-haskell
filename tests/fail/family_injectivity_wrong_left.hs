{-# LANGUAGE TypeFamilyDependencies #-}

import Data.Kind (Type)

type family F a = r | a -> a

main :: IO ()
main = pure ()
