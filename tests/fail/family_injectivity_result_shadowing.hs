{-# LANGUAGE TypeFamilyDependencies #-}

import Data.Kind (Type)

type family F (a :: k) = k | k -> a

main :: IO ()
main = pure ()
