{-# LANGUAGE NoPolyKinds, RoleAnnotations, NoEmptyDataDecls #-}
module CacheBootGndContext where
data N a
type role N representational
instance Eq a => Eq (N a)
makeN :: a -> N a
