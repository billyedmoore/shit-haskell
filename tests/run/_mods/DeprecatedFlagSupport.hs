{-# LANGUAGE RecordPuns #-}
module DeprecatedFlagSupport (wrapped) where

data Wrapper = Wrapper { value :: Int }

unwrap :: Wrapper -> Int
unwrap Wrapper {value} = value

wrapped :: Int
wrapped = unwrap (Wrapper 7)
