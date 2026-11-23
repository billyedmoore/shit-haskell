{-# LANGUAGE NoDataKinds, UnboxedTuples #-}

import Data.Kind (Type)

data D (a :: (# Type, Type #)) = D

main :: IO ()
main = pure ()
