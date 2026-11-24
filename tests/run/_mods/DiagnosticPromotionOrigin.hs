{-# LANGUAGE DataKinds #-}
module DiagnosticPromotionOrigin where
data Tree = Tree :*: Tree | Leaf
type L = 'Leaf
type Nil = '[]
