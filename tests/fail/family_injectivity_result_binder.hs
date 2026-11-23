{-# LANGUAGE TypeFamilyDependencies #-}

import Data.Kind (Type)

type family F a = Int | Int -> a

main :: IO ()
main = pure ()
