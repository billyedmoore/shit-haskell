{-# LANGUAGE PatternSynonyms #-}
module NamespaceOrigin (T(.., Other)) where

data T = Here

pattern Other :: T
pattern Other = Here
