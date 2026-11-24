{-# LANGUAGE NoKindSignatures #-}

import Data.Kind (Type)

value :: (Int :: Type)
value = 1

main :: IO ()
main = pure ()
